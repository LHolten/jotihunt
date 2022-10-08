use std::collections::BTreeMap;

use crate::state::{Address, Fox};
use futures::{self, channel::mpsc, future, StreamExt, TryStreamExt};
use gloo::net::websocket::{futures::WebSocket, Message};
use jotihunt_client::update::{AtomicEdit, Broadcast};
use sycamore::reactive::Signal;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/leaflet.js")]
extern "C" {
    type Marker;

    fn add_marker(lat: f64, lng: f64, name: String) -> Marker;
    #[wasm_bindgen(js_name = remove_layer)]
    fn remove_marker(marker: Marker);
    fn set_marker_color(marker: &Marker, last: bool);

    type Line;

    fn new_line() -> Line;
    fn add_line_marker(line: &Line, marker: &Marker) -> Line;
    #[wasm_bindgen(js_name = remove_layer)]
    fn remove_line(line: Line);
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
    let mut lines = BTreeMap::new();

    read.try_for_each(|msg| match msg {
        Message::Text(_) => panic!("we want bytes"),
        Message::Bytes(bin) => {
            let broadcast: Broadcast = postcard::from_bytes(&bin).unwrap();
            let key: Address = postcard::from_bytes(&broadcast.key).unwrap();
            last_marker_color(&markers, &key.fox_name, false);
            if let Some(old_marker) = markers.remove(&key) {
                remove_marker(old_marker);
            }
            if let Some(old_line) = lines.remove(&key.fox_name) {
                remove_line(old_line)
            }
            if broadcast.value.is_empty() {
                data.modify().remove(&key);
            } else {
                let fox: Fox = postcard::from_bytes(&broadcast.value).unwrap();
                let name = format!("{} ({})", key.fox_name, key.time_slice);
                if let Some(marker) = make_marker(&fox, name) {
                    markers.insert(key.clone(), marker);
                }
                let line = new_line();
                for (k, v) in &markers {
                    if &k.fox_name == &key.fox_name {
                        add_line_marker(&line, v);
                    }
                }
                lines.insert(key.fox_name.clone(), line);

                data.modify().insert(key.clone(), fox);
            }
            last_marker_color(&markers, &key.fox_name, true);
            future::ok(())
        }
    })
    .await
    .unwrap();
}

fn last_marker_color<'a>(markers: &'a BTreeMap<Address, Marker>, fox_name: &str, last: bool) {
    if let Some((_a, m)) = markers
        .into_iter()
        .rev()
        .find(|(a, _)| a.fox_name == fox_name)
    {
        set_marker_color(m, last)
    }
}

fn make_marker(fox: &Fox, name: String) -> Option<Marker> {
    make_value(&fox.latitude)
        .zip(make_value(&fox.longitude))
        .map(|(lat, lng)| add_marker(lat, lng, name))
}

fn make_value(input: &str) -> Option<f64> {
    // log!("{}", input);
    let Ok(val) = input.parse::<u32>() else {return None};
    if input.len() != 4 {
        return None;
    };
    Some((val * 100) as f64)
}
