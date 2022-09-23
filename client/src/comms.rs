use crate::state::{Address, Fox};
use futures::{self, channel::mpsc, future, StreamExt, TryStreamExt};
use gloo::net::websocket::{futures::WebSocket, Message};
use jotihunt_client::update::{AtomicEdit, Broadcast};
use sycamore::reactive::Signal;

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
    read.try_for_each(|msg| match msg {
        Message::Text(_) => panic!("we want bytes"),
        Message::Bytes(bin) => {
            let broadcast: Broadcast = postcard::from_bytes(&bin).unwrap();
            if broadcast.value.is_empty() {
                data.modify()
                    .remove(&postcard::from_bytes(&broadcast.key).unwrap());
            } else {
                data.modify().insert(
                    postcard::from_bytes(&broadcast.key).unwrap(),
                    postcard::from_bytes(&broadcast.value).unwrap(),
                );
            }
            future::ok(())
        }
    })
    .await
    .unwrap();
}
