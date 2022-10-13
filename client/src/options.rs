use std::{collections::HashMap, future::ready};

use futures::{channel::oneshot, FutureExt, StreamExt};
use gloo::{
    console::console_dbg,
    net::websocket::{futures::WebSocket, Message},
    timers::future::TimeoutFuture,
    utils::document,
};
use jotihunt_shared::Traccar;
use sycamore::{futures::spawn_local_scoped, prelude::*};

use crate::{leaflet::Marker, state::Fox, HOSTNAME, WS_PROTOCOL};

pub fn option_panel(key: &'static str) {
    let panel = document()
        .get_element_by_id("option_panel")
        .expect("there is a add_point button");

    sycamore::render_to(
        |cx| {
            let show_live = create_signal(cx, false);

            create_effect_scoped(cx, move |cx| {
                if *show_live.get() {
                    let ws_address = format!("{WS_PROTOCOL}://{HOSTNAME}/live/{key}");
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
    let mut live_data = HashMap::new();

    ws.for_each_concurrent(None, |m| {
        let traccar: Traccar = match m.unwrap() {
            Message::Text(_) => panic!("we want bytes"),
            Message::Bytes(bin) => postcard::from_bytes(&bin).unwrap(),
        };
        console_dbg!(&traccar);
        let live_loc = Fox {
            latitude: traccar.lat,
            longitude: traccar.lon,
        };
        let Some(marker) = make_marker(&live_loc, traccar.id.clone()) else {
            return ready(()).boxed_local();
        };
        console_dbg!("placed marker");
        let (mut send, receive) = oneshot::channel::<()>();
        live_data.insert(traccar.id, receive);

        async move {
            futures::select! {
                _ = TimeoutFuture::new(5 * 60 * 1000).fuse() => (),
                _ = send.cancellation().fuse() => ()
            }
            console_dbg!("removed marker");
            drop(marker)
        }
        .boxed_local()
    })
    .await
}

pub fn make_marker(fox: &Fox, name: String) -> Option<Marker> {
    Some(Marker::new(
        fox.longitude.parse().ok()?,
        fox.latitude.parse().ok()?,
        name,
        false,
    ))
}
