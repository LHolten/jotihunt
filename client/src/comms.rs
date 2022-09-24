use std::collections::BTreeMap;

use crate::state::{Address, Fox};
use futures::{self, channel::mpsc, future, StreamExt, TryStreamExt};
use gloo::{
    console::log,
    net::websocket::{futures::WebSocket, Message},
};
use jotihunt_client::update::{AtomicEdit, Broadcast};
use sycamore::reactive::Signal;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/leaflet.js")]
extern "C" {
    type Map;

    fn make_map() -> Map;

    type Marker;

    fn add_marker(map: &Map, lat: f64, lng: f64) -> Marker;
    fn remove_marker(map: &Map, marker: Marker);
}

pub(crate) async fn write_data(
    queue_read: mpsc::UnboundedReceiver<AtomicEdit>,
    write: futures::stream::SplitSink<WebSocket, Message>,
) {
    queue_read
        .map(|edit| {
            let msg = Message::Bytes(postcard::to_stdvec(&edit).unwrap());
            Ok(msg)
        })
        .forward(write)
        .await
        .unwrap();
}

pub(crate) async fn read_data(
    read: futures::stream::SplitStream<WebSocket>,
    data: &Signal<std::collections::BTreeMap<Address, Fox>>,
) {
    let mut markers = BTreeMap::new();
    let map = make_map();

    read.try_for_each(|msg| match msg {
        Message::Text(_) => panic!("we want bytes"),
        Message::Bytes(bin) => {
            let broadcast: Broadcast = postcard::from_bytes(&bin).unwrap();
            let key = postcard::from_bytes(&broadcast.key).unwrap();
            if let Some(old_marker) = markers.remove(&key) {
                remove_marker(&map, old_marker);
            }
            if broadcast.value.is_empty() {
                data.modify().remove(&key);
            } else {
                let fox: Fox = postcard::from_bytes(&broadcast.value).unwrap();
                if let Some(marker) = make_marker(&fox, &map) {
                    markers.insert(key.clone(), marker);
                }
                data.modify().insert(key, fox);
            }
            future::ok(())
        }
    })
    .await
    .unwrap();
}

fn make_marker(fox: &Fox, map: &Map) -> Option<Marker> {
    make_value(&fox.latitude)
        .zip(make_value(&fox.longitude))
        .map(|(lat, lng)| add_marker(map, lat, lng))
}

fn make_value(input: &str) -> Option<f64> {
    // log!("{}", input);
    let Ok(val) = input.parse::<u32>() else {return None};
    if input.len() != 4 {
        return None;
    };
    Some((val * 100) as f64)
}
