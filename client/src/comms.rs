use std::collections::BTreeMap;

use crate::leaflet::Marker;
use crate::HOSTNAME;
use futures::{self, channel::mpsc, stream, Stream, StreamExt};
use gloo::{dialogs::alert, events::EventListener, net::http::Request, utils::document};
use jotihunt_shared::domain::{Fox, FoxKey};
use jotihunt_shared::{Broadcast, ModifyLocation};
use js_sys::{encode_uri_component, Uint8Array};
use serde::de::DeserializeOwned;
use sycamore::{futures::spawn_local_scoped, prelude::*};
use wasm_bindgen::JsCast;
use web_sys::{Event, EventSource, MessageEvent};

pub enum LocationEdit {
    Create(FoxKey, Fox),
    Modify(FoxKey, ModifyLocation),
    /// the location is only deleted if it is still the given [Fox]
    Delete(FoxKey, Fox),
}

// edits are sent one at a time, so that they are applied in order
async fn write_data(mut queue_read: mpsc::UnboundedReceiver<LocationEdit>, address: String) {
    while let Some(edit) = queue_read.next().await {
        let path = |key: &FoxKey| {
            let [day, time, fox_name] =
                [&key.day, &key.time, &key.fox_name].map(|s| String::from(encode_uri_component(s)));
            format!("{address}/{day}/{time}/{fox_name}")
        };
        let (req, bin, create) = match edit {
            LocationEdit::Create(key, fox) => {
                (Request::post(&path(&key)), serde_json::to_vec(&fox), true)
            }
            LocationEdit::Modify(key, edit) => {
                (Request::put(&path(&key)), serde_json::to_vec(&edit), false)
            }
            LocationEdit::Delete(key, old) => (
                Request::delete(&path(&key)),
                serde_json::to_vec(&old),
                false,
            ),
        };
        let res = match req.body(Uint8Array::from(&bin.unwrap()[..])) {
            Ok(req) => req.send().await,
            Err(err) => Err(err),
        };
        match res {
            // 409 (and 404 when modifying or deleting) means the value was changed by someone else,
            // just like before we ignore it
            Ok(res) if res.ok() || res.status() == 409 || (!create && res.status() == 404) => {}
            Ok(res) => alert(&format!("opslaan mislukt: {}", res.status_text())),
            Err(err) => alert(&format!("opslaan mislukt: {err}")),
        }
    }
}

/// How the event streams are connected, shown in the banner.
#[derive(Clone, Copy)]
struct ConnectionStatus {
    /// the number of event streams that lost their connection
    disconnected: Signal<u32>,
    /// whether the browser gave up on an event stream, then only reopening the page helps
    gave_up: Signal<bool>,
}

thread_local! {
    /// Created in the root scope, so it lives on when the scope that first uses it is gone.
    static STATUS: ConnectionStatus = use_global_scope().run_in(connection_banner);
}

fn status() -> ConnectionStatus {
    STATUS.with(|status| *status)
}

/// Shows a banner while an event stream lost its connection.
fn connection_banner() -> ConnectionStatus {
    let banner = document()
        .get_element_by_id("offline_banner")
        .expect("there is an offline banner");

    let disconnected = create_signal(0);
    let gave_up = create_signal(false);
    sycamore::render_in_scope(
        move || {
            // the banner is hidden while it is empty
            view! {
                (if gave_up.get() {
                    view! { "Kan geen verbiding maken met de server" }
                } else if disconnected.get() > 0 {
                    view! { "Aan het proberen opnieuw verbinding te maken met de server" }
                } else {
                    view! {}
                })
            }
        },
        &banner,
    );
    ConnectionStatus {
        disconnected,
        gave_up,
    }
}

/// Counts as a lost connection for the banner while it exists.
struct Disconnected;

impl Disconnected {
    fn new() -> Self {
        status().disconnected.update(|count| *count += 1);
        Disconnected
    }
}

impl Drop for Disconnected {
    fn drop(&mut self) {
        status().disconnected.update(|count| *count -= 1);
    }
}

enum SourceEvent {
    Open,
    Message(String),
    Error,
}

/// One connection to the server sent events at an address,
/// it is closed when this is dropped.
struct Connection {
    source: EventSource,
    events: mpsc::UnboundedReceiver<SourceEvent>,
    // the listeners are removed when they are dropped
    _listeners: [EventListener; 3],
}

impl Connection {
    fn new(address: &str) -> Self {
        let source = EventSource::new(address).unwrap();
        let (sender, events) = mpsc::unbounded();
        let listen = |event: &'static str, make: fn(&Event) -> SourceEvent| {
            let sender = sender.clone();
            EventListener::new(&source, event, move |e| {
                let _ = sender.unbounded_send(make(e));
            })
        };
        let _listeners = [
            listen("open", |_| SourceEvent::Open),
            listen("message", |e| {
                let data = e.unchecked_ref::<MessageEvent>().data();
                SourceEvent::Message(data.as_string().unwrap())
            }),
            listen("error", |_| SourceEvent::Error),
        ];
        Connection {
            source,
            events,
            _listeners,
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.source.close();
    }
}

/// The data of all messages from the server sent events at `address`.
///
/// The browser reconnects by itself after most errors, but it gives up when the
/// server responds with an error (e.g. a 502 while it restarts). The stream then ends
/// and the banner asks to reopen the page.
pub fn event_messages(address: String) -> impl Stream<Item = String> {
    let state = (Connection::new(&address), None::<Disconnected>);
    stream::unfold(state, |(mut connection, mut disconnected)| async move {
        loop {
            match connection.events.next().await.unwrap() {
                SourceEvent::Open => disconnected = None,
                SourceEvent::Message(data) => {
                    return Some((data, (connection, disconnected)));
                }
                // also happens in chrome when the page is refreshed,
                // the banner is only visible for a moment then
                SourceEvent::Error if connection.source.ready_state() == EventSource::CLOSED => {
                    status().gave_up.set(true);
                    return None;
                }
                SourceEvent::Error => {
                    disconnected.get_or_insert_with(Disconnected::new);
                }
            }
        }
    })
}

async fn read_data<K, V>(address: String, data: Signal<BTreeMap<K, V>>)
where
    K: DeserializeOwned + Clone + Ord,
    V: DeserializeOwned + Clone,
{
    // after every reconnect by the browser the server starts with a reset followed by all rows
    let mut messages = Box::pin(event_messages(address));
    while let Some(json) = messages.next().await {
        match serde_json::from_str(&json).unwrap() {
            Broadcast::Reset => data.set(BTreeMap::new()),
            Broadcast::Update {
                key,
                value: Some(value),
            } => {
                data.update(|data| data.insert(key, value));
            }
            Broadcast::Update { key, value: None } => {
                data.update(|data| data.remove(&key));
            }
        }
    }
}

/// All rows of the table with server sent events at `address`, kept up to date.
pub fn live_table<K, V>(address: String) -> ReadSignal<BTreeMap<K, V>>
where
    K: DeserializeOwned + Clone + Ord + 'static,
    V: DeserializeOwned + Clone + 'static,
{
    let data = create_signal(BTreeMap::<K, V>::new());
    spawn_local_scoped(read_data(address, data));
    *data
}

/// Like [live_table], but also returns a function to edit the table.
pub fn live_updated<K, V>(
    key: &str,
    name: &str,
) -> (
    ReadSignal<BTreeMap<K, V>>,
    impl Fn(LocationEdit) + Copy + 'static,
)
where
    K: DeserializeOwned + Clone + Ord + 'static,
    V: DeserializeOwned + Clone + 'static,
{
    let address = format!("https://{HOSTNAME}/{key}/{name}");

    let (queue_write, queue_read) = mpsc::unbounded();

    spawn_local_scoped(write_data(queue_read, address.clone()));
    let data = live_table(address);
    // the signal is only used as a `Copy` handle to the sender
    let queue_write = create_signal(queue_write);
    let send = move |edit| queue_write.get_clone().unbounded_send(edit).unwrap();
    (data, send)
}

// creates a marker if both coordinates are valid
pub fn make_marker(fox: &Fox, name: &str) -> Option<Marker> {
    let (x, y, convert) = parse_coord(&fox.latitude, &fox.longitude)?;
    Some(Marker::new(x, y, name.to_owned(), convert))
}

/// Whether both coordinates are valid, so a marker can be made.
pub fn is_valid(fox: &Fox) -> bool {
    parse_coord(&fox.latitude, &fox.longitude).is_some()
}

// parses both sides of a coordinate, returns the marker position and if it is RD
// RD needs both sides, otherwise each side can be decimal or degrees minutes seconds
fn parse_coord(lat: &str, long: &str) -> Option<(f64, f64, bool)> {
    fn try_rd(lat: &str, long: &str) -> Option<(f64, f64, bool)> {
        Some((make_value(lat)?, make_value(long)?, true))
    }
    fn degrees(part: &str, pos: &str, neg: &str, max: f64) -> Option<f64> {
        part.parse().ok().or_else(|| parse_dms(part, pos, neg, max))
    }
    try_rd(lat, long).or_else(|| {
        Some((
            degrees(long, "E", "W", 180.0)?,
            degrees(lat, "N", "S", 90.0)?,
            false,
        ))
    })
}

// checks if the given value is a valid 4 digit number
fn make_value(input: &str) -> Option<f64> {
    let Ok(val) = input.parse::<u32>() else {
        return None;
    };
    if input.len() != 4 {
        return None;
    };
    Some((val * 100) as f64)
}

// parses a single degrees, minutes, seconds value like `52°00'35.4"N`
fn parse_dms(part: &str, pos: &str, neg: &str, max: f64) -> Option<f64> {
    let (a, part) = part.split_once('°')?;
    let (b, part) = part.split_once('\'')?;
    let (c, end) = part.split_once('"')?;
    let a = a.parse::<u32>().ok()?;
    let b = b.parse::<u32>().ok()?;
    let c = c.parse::<f64>().ok()?;

    let sign = match end {
        e if e == pos => 1.0,
        e if e == neg => -1.0,
        _ => return None,
    };
    if 60 <= b {
        return None;
    }
    if c < 0. || 60. <= c {
        return None;
    }

    let value = a as f64 + b as f64 / 60. + c / 3600.;

    (value <= max).then_some(sign * value)
}

#[test]
fn parse() {
    let (x, y, rd) = parse_coord(r#"52°00'35.4"N"#, r#"4°25'08.7"E"#).unwrap();
    assert!(!rd);
    assert!((y - 52.009833).abs() < 1e-6);
    assert!((x - 4.419083).abs() < 1e-6);
    let (x, y, _) = parse_coord(r#"52°00'35.4"S"#, r#"4°25'08.7"W"#).unwrap();
    assert!(x < 0.0 && y < 0.0);
    assert_eq!(
        parse_coord("51.95", r#"4°25'08.7"E"#).map(|c| c.1),
        Some(51.95)
    );
    assert_eq!(
        parse_coord("1234", "5678"),
        Some((123400.0, 567800.0, true))
    );
    assert!(parse_coord("abc", "5.87").is_none());
}
