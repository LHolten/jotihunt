use std::collections::BTreeMap;

use crate::leaflet::Marker;
use crate::{HOSTNAME, WS_PROTOCOL};
use futures::{
    self,
    channel::mpsc::{self, UnboundedSender},
    future, StreamExt, TryStreamExt,
};
use gloo::{
    dialogs::alert,
    net::websocket::{futures::WebSocket, Message},
};
use jotihunt_shared::domain::Fox;
use jotihunt_shared::{AtomicEdit, Broadcast};
use js_sys::Date;
use serde::de::DeserializeOwned;
use sycamore::{
    futures::spawn_local_scoped,
    prelude::{create_ref, create_signal, BoundedScope, ReadSignal},
    reactive::Signal,
};

async fn write_data(
    queue_read: mpsc::UnboundedReceiver<AtomicEdit>,
    write: futures::stream::SplitSink<WebSocket, Message>,
) {
    let _ = queue_read
        .map(|edit| {
            let msg = Message::Bytes(postcard::to_stdvec(&edit).unwrap());
            Ok(msg)
        })
        .forward(write)
        .await;
}

async fn read_data<K, V>(
    read: futures::stream::SplitStream<WebSocket>,
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
    &'cx UnboundedSender<AtomicEdit>,
)
where
    K: DeserializeOwned + Clone + Ord,
    V: DeserializeOwned + Clone,
{
    let data = create_signal(cx, BTreeMap::<K, V>::new());

    let ws_address = format!("{WS_PROTOCOL}://{HOSTNAME}/{key}/{name}");
    let ws = WebSocket::open(&ws_address).unwrap();

    let (write, read) = ws.split();
    let (queue_write, queue_read) = mpsc::unbounded();

    spawn_local_scoped(cx, write_data(queue_read, write));
    spawn_local_scoped(cx, read_data(read, data));
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
