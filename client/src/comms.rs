use std::collections::BTreeMap;

use crate::{
    leaflet::{Line, Marker},
    state::{Address, Fox},
};
use futures::{self, channel::mpsc, future, StreamExt, TryStreamExt};
use gloo::{
    dialogs::alert,
    net::websocket::{futures::WebSocket, Message},
};
use jotihunt_shared::{AtomicEdit, Broadcast};
use js_sys::Date;
use sycamore::reactive::Signal;

pub(crate) async fn write_data(
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

pub(crate) async fn read_data(
    read: futures::stream::SplitStream<WebSocket>,
    data: &Signal<std::collections::BTreeMap<Address, Fox>>,
) {
    let mut markers = BTreeMap::new();
    let mut lines = BTreeMap::new();

    let _ = read
        .try_for_each(|msg| match msg {
            Message::Text(_) => panic!("we want bytes"),
            Message::Bytes(bin) => {
                let broadcast: Broadcast = postcard::from_bytes(&bin).unwrap();
                let key: Address = postcard::from_bytes(&broadcast.key).unwrap();
                last_marker_color(&markers, &key.fox_name, "yellow");
                markers.remove(&key);

                if broadcast.value.is_empty() {
                    data.modify().remove(&key);
                } else {
                    let fox: Fox = postcard::from_bytes(&broadcast.value).unwrap();
                    let name = format!("{} ({})", key.fox_name, key.time_slice);
                    if let Some(marker) = make_marker(&fox, &name) {
                        marker.set_color("yellow");
                        markers.insert(key.clone(), marker);
                    }
                    data.modify().insert(key.clone(), fox);
                }

                // build a line from all timestamps for this fox
                let line = Line::new();
                for (k, v) in &markers {
                    if k.fox_name == key.fox_name {
                        line.push(v);
                    }
                }
                // replace any line with the same fox name
                lines.insert(key.fox_name.clone(), line);

                last_marker_color(&markers, &key.fox_name, "orange");
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

// change the color of the last marker of the given name
fn last_marker_color(markers: &BTreeMap<Address, Marker>, fox_name: &str, color: &str) {
    if let Some((_a, m)) = markers.iter().rev().find(|(a, _)| a.fox_name == fox_name) {
        m.set_color(color)
    }
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
