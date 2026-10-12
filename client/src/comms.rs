use std::{collections::BTreeMap, future::ready};

use crate::leaflet::Marker;
use crate::HOSTNAME;
use futures::{self, channel::mpsc, Stream, StreamExt};
use gloo::{
    dialogs::alert,
    net::{eventsource::futures::EventSource, http::Request},
};
use jotihunt_shared::domain::{Fox, FoxKey};
use jotihunt_shared::{Broadcast, ModifyLocation};
use js_sys::{encode_uri_component, Date, Uint8Array};
use serde::de::DeserializeOwned;
use sycamore::{
    futures::spawn_local_scoped,
    reactive::{create_signal, ReadSignal, Signal},
};

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

/// The data of all messages from the server sent events at `address`.
///
/// The browser reconnects by itself after most errors, but it gives up when the
/// server responds with an error (e.g. a 502 while it restarts). The stream then ends.
pub fn event_messages(address: String) -> impl Stream<Item = String> {
    // the connection is closed when the `EventSource` is dropped,
    // so it is moved into the stream
    let mut events = EventSource::new(&address).unwrap();
    let messages = events.subscribe("message").unwrap();
    // errors are only reported once the browser gave up
    messages
        .take_while(|msg| ready(msg.is_ok()))
        .map(move |msg| {
            let _keep_open = &events;
            msg.unwrap().1.data().as_string().unwrap()
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

    let local_time = Date::new_0().to_time_string();
    let msg = format!(
        "verbinding verbroken: ververs de pagina voor nieuwe data!
        {local_time}"
    );
    alert(&msg)
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
    let send = move |edit| queue_write.with(|queue| queue.unbounded_send(edit).unwrap());
    (data, send)
}

// creates a marker if both coordinates are valid
pub fn make_marker(fox: &Fox, name: &str) -> Option<Marker> {
    let (x, y, convert) = parse_coord(&fox.latitude, &fox.longitude)?;
    Some(Marker::new(x, y, name.to_owned(), convert))
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
