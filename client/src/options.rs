use std::{collections::HashMap, future::ready};

use futures::{channel::oneshot, FutureExt, StreamExt, TryStreamExt};
use gloo::{console::console_dbg, dialogs::alert, timers::future::TimeoutFuture, utils::document};
use jotihunt_shared::{domain::Fox, Traccar};
use mk_geolocation::{future::PositionStream, PositionOptions};
use sycamore::{futures::spawn_local_scoped, prelude::*};

use crate::{comms::event_messages, leaflet::Marker, HOSTNAME};

pub fn option_panel(key: &'static str) {
    let panel = document()
        .get_element_by_id("option_panel")
        .expect("there is a add_point button");

    sycamore::render_to(
        move || {
            let show_live = create_signal(true);

            create_effect(move || {
                if show_live.get() {
                    let address = format!("https://{HOSTNAME}/{key}/live");
                    spawn_local_scoped(read_live(address))
                }
            });

            let show_me = create_signal(false);

            create_effect(move || {
                if show_me.get() {
                    spawn_local_scoped(my_loc())
                }
            });

            view! {
                summary {"Opties"}
                div {"Deze url invullen in je traccar client:"}
                pre {(format!("https://{HOSTNAME}/traccar"))}
                div(class="field"){
                    label(r#for="traccar"){"Traccar bekijken:"}
                    input(id="traccar", r#type="checkbox", bind:checked=show_live)
                    label(r#for="mijn"){"Mijn locatie:"}
                    input(id="mijn", r#type="checkbox", bind:checked=show_me)
                }
            }
        },
        &panel,
    );
}

async fn my_loc() {
    let mut options = PositionOptions::new();
    options.enable_high_accuracy(true);
    let _ = PositionStream::new_with_options(options)
        .try_for_each(move |pos| {
            let coords = pos.coords();
            let m = Marker::new(
                coords.longitude(),
                coords.latitude(),
                "you".to_string(),
                false,
            );
            m.zoom_to();
            ready(Ok(()))
        })
        .await;
    alert("could not get your location")
}

async fn read_live(address: String) {
    let mut live_data = HashMap::new();

    event_messages(address)
        .for_each_concurrent(None, |json| {
            let traccar: Traccar = serde_json::from_str(&json).unwrap();
            console_dbg!(&traccar);
            let live_loc = Fox {
                latitude: traccar.lat,
                longitude: traccar.lon,
            };
            let Some(marker) = make_marker(&live_loc, traccar.id.clone()) else {
                return ready(()).boxed_local();
            };
            marker.set_human();
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
