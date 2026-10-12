use std::{collections::BTreeMap, future::ready};

use crate::leaflet::Marker;
use crate::HOSTNAME;
use futures::{
    self,
    channel::mpsc::{self, UnboundedSender},
    Stream, StreamExt,
};
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
    prelude::{create_ref, create_signal, BoundedScope, ReadSignal},
    reactive::Signal,
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

async fn read_data<K, V>(address: String, data: &Signal<std::collections::BTreeMap<K, V>>)
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
                data.modify().insert(key, value);
            }
            Broadcast::Update { key, value: None } => {
                data.modify().remove(&key);
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
pub fn live_table<'cx, K, V>(
    cx: BoundedScope<'cx, 'cx>,
    address: String,
) -> &'cx ReadSignal<BTreeMap<K, V>>
where
    K: DeserializeOwned + Clone + Ord,
    V: DeserializeOwned + Clone,
{
    let data = create_signal(cx, BTreeMap::<K, V>::new());
    spawn_local_scoped(cx, read_data(address, data));
    data
}

pub fn live_updated<'cx, K, V>(
    cx: BoundedScope<'cx, 'cx>,
    key: &str,
    name: &str,
) -> (
    &'cx ReadSignal<BTreeMap<K, V>>,
    &'cx UnboundedSender<LocationEdit>,
)
where
    K: DeserializeOwned + Clone + Ord,
    V: DeserializeOwned + Clone,
{
    let address = format!("https://{HOSTNAME}/{key}/{name}");

    let (queue_write, queue_read) = mpsc::unbounded();

    spawn_local_scoped(cx, write_data(queue_read, address.clone()));
    let data = live_table(cx, address);
    (data, create_ref(cx, queue_write))
}

// creates a marker if both coordinates are valid
// first tries converting fom RD, then accepts lat long
pub fn make_marker(fox: &Fox, name: &str) -> Option<Marker> {
    fn try_rd(fox: &Fox, name: &str) -> Option<Marker> {
        Some(Marker::new(
            make_value(&fox.latitude)?,
            make_value(&fox.longitude)?,
            name.to_owned(),
            true,
        ))
    }
    try_rd(fox, name).or_else(|| {
        Some(Marker::new(
            fox.longitude.parse().ok()?,
            fox.latitude.parse().ok()?,
            name.to_owned(),
            false,
        ))
    })
}

// checks if the given value is a valid 4 digit number
fn make_value(input: &str) -> Option<f64> {
    // log!("{}", input);
    let Ok(val) = input.parse::<u32>() else {
        return None;
    };
    if input.len() != 4 {
        return None;
    };
    Some((val * 100) as f64)
}
