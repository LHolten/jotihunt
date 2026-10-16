use std::{collections::HashMap, future::ready};

use futures::{StreamExt, TryStreamExt};
use gloo::{dialogs::alert, timers::future::IntervalStream, utils::document};
use jotihunt_shared::{domain::Fox, Traccar};
use js_sys::Date;
use mk_geolocation::{future::PositionStream, PositionOptions};
use qrcode::{render::svg, QrCode};
use sycamore::{futures::spawn_local_scoped, prelude::*};

use crate::{comms::event_messages, leaflet::Marker, HOSTNAME};

/// Seconds between traccar updates of a phone that is standing still,
/// markers without an update for this long are removed from the map.
const HEARTBEAT_SECS: u32 = 5 * 60;

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

            let traccar_url = format!("https://{HOSTNAME}/traccar");
            // the traccar client reads these settings from the url
            let traccar_config = format!(
                "{traccar_url}?accuracy=high&distance=15&heartbeat={HEARTBEAT_SECS}&stop_detection=true&buffer=false"
            );
            // for scanning with a phone, the quiet zone keeps it readable on the green panel
            let traccar_qr = QrCode::new(&traccar_config)
                .unwrap()
                .render::<svg::Color>()
                .min_dimensions(160, 160)
                .build();

            view! {
                summary {"Opties"}
                ol {
                    li {
                        "Installeer de "
                        a(href="https://www.traccar.org/client/", target="_blank") {"Traccar Client-app"}
                    }
                    li {"Open de app, ga naar Settings en scan de QR-code"}
                    li {"Stel bij \"Device identifier\" je naam in"}
                }
                pre {(traccar_url)}
                div(dangerously_set_inner_html=traccar_qr)
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
    // the marker is removed from the map when it is dropped,
    // so it is kept until the next position replaces it
    let mut marker = None;
    let _ = PositionStream::new_with_options(options)
        .try_for_each(|pos| {
            let coords = pos.coords();
            let m = Marker::new(
                coords.longitude(),
                coords.latitude(),
                "you".to_string(),
                false,
            );
            m.zoom_to();
            marker = Some(m);
            ready(Ok(()))
        })
        .await;
    alert("could not get your location")
}

async fn read_live(address: String) {
    // markers are removed from the map when they are dropped
    let mut markers = HashMap::new();
    let mut messages = Box::pin(event_messages(address)).fuse();
    let mut prune = IntervalStream::new(60 * 1000).fuse();
    loop {
        futures::select! {
            json = messages.next() => {
                let Some(json) = json else { break };
                let traccar: Traccar = serde_json::from_str(&json).unwrap();
                let live_loc = Fox {
                    latitude: traccar.lat,
                    longitude: traccar.lon,
                };
                if let Some(marker) = make_marker(&live_loc, traccar.id.clone()) {
                    marker.set_human();
                    markers.insert(traccar.id, (marker, Date::now()));
                }
            }
            _ = prune.next() => {
                let now = Date::now();
                markers.retain(|_, (_, updated)| now - *updated < f64::from(HEARTBEAT_SECS * 1000));
            }
        }
    }
}

pub fn make_marker(fox: &Fox, name: String) -> Option<Marker> {
    Some(Marker::new(
        fox.longitude.parse().ok()?,
        fox.latitude.parse().ok()?,
        name,
        false,
    ))
}
