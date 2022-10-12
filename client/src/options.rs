use futures::StreamExt;
use gloo::{
    net::websocket::{futures::WebSocket, Message},
    utils::document,
};
use jotihunt_shared::Traccar;
use sycamore::{futures::spawn_local_scoped, prelude::*};

use crate::{leaflet::Marker, HOSTNAME, WS_PROTOCOL};

pub fn option_panel(key: &'static str) {
    let panel = document()
        .get_element_by_id("option_panel")
        .expect("there is a add_point button");

    sycamore::render_to(
        |cx| {
            let show_live = create_signal(cx, false);

            create_effect_scoped(cx, move |cx| {
                if *show_live.get() {
                    let ws_address = format!("{WS_PROTOCOL}://{HOSTNAME}/{key}");
                    let ws = WebSocket::open(&ws_address).unwrap();
                    spawn_local_scoped(cx, read_live(ws))
                }
            });

            view! {cx,
                div(class="field"){
                    label(for="traccar"){"Traccar:"}
                    input(id="traccar", type="checkbox", bind:checked=show_live)
                }
            }
        },
        &panel,
    );
}

async fn read_live(ws: WebSocket) {
    ws.for_each_concurrent(None, |m| async {
        let traccar: Traccar = match m.unwrap() {
            Message::Text(_) => panic!("we want bytes"),
            Message::Bytes(bin) => postcard::from_bytes(&bin).unwrap(),
        };
        // let marker = Marker::new();
    })
    .await
}
