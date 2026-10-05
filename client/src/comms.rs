use std::collections::BTreeMap;

use crate::leaflet::Marker;
use crate::{HOSTNAME, HTTP_PROTOCOL, WS_PROTOCOL};
use futures::{
    self,
    channel::mpsc::{self, UnboundedSender},
    future, StreamExt, TryStreamExt,
};
use gloo::{
    dialogs::alert,
    net::{
        http::Request,
        websocket::{futures::WebSocket, Message},
    },
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
                (Request::post(&path(&key)), postcard::to_stdvec(&fox), true)
            }
            LocationEdit::Modify(key, edit) => {
                (Request::put(&path(&key)), postcard::to_stdvec(&edit), false)
            }
            LocationEdit::Delete(key, old) => {
                (Request::delete(&path(&key)), postcard::to_stdvec(&old), false)
            }
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

async fn read_data<K, V>(
    read: WebSocket,
    data: &Signal<std::collections::BTreeMap<K, V>>,
) where
    K: DeserializeOwned + Clone + Ord,
    V: DeserializeOwned + Clone,
{
    let _ = read
        .try_for_each(|msg| match msg {
            Message::Text(_) => panic!("we want bytes"),
            Message::Bytes(bin) => {
                let broadcast: Broadcast = postcard::from_bytes(&bin).unwrap();
                let key: K = postcard::from_bytes(&broadcast.key).unwrap();

                if broadcast.value.is_empty() {
                    data.modify().remove(&key);
                } else {
                    let fox: V = postcard::from_bytes(&broadcast.value).unwrap();
                    data.modify().insert(key.clone(), fox);
                }
                future::ok(())
            }
        })
        .await;

    let local_time = Date::new_0().to_time_string();
    let msg = format!(
        "verbinding verbroken: ververs de pagina voor nieuwe data!
        {local_time}"
    );
    alert(&msg)
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
    let data = create_signal(cx, BTreeMap::<K, V>::new());

    let ws_address = format!("{WS_PROTOCOL}://{HOSTNAME}/{key}/{name}");
    let ws = WebSocket::open(&ws_address).unwrap();
    let post_address = format!("{HTTP_PROTOCOL}://{HOSTNAME}/{key}/{name}");

    let (queue_write, queue_read) = mpsc::unbounded();

    spawn_local_scoped(cx, write_data(queue_read, post_address));
    spawn_local_scoped(cx, read_data(ws, data));
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
