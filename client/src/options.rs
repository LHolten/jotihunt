use std::{collections::HashMap, future::ready};

use futures::{StreamExt, TryStreamExt};
use gloo::{
    dialogs::alert,
    storage::{LocalStorage, Storage},
    timers::future::IntervalStream,
    utils::document,
};
use jotihunt_shared::{domain::Fox, Traccar};
use js_sys::Date;
use mk_geolocation::{future::PositionStream, PositionOptions};
use qrcode::{render::svg, QrCode};
use sycamore::{futures::spawn_local_scoped, prelude::*};

use crate::{
    comms::event_messages,
    leaflet::{set_group_names, Marker},
    HOSTNAME,
};

/// Seconds between traccar updates of a phone that is standing still,
/// markers without an update for this long are removed from the map.
const HEARTBEAT_SECS: u32 = 5 * 60;

const GROUP_NAMES_KEY: &str = "group_names";
const GROUP_NAMES_ZOOM_KEY: &str = "group_names_zoom";

pub fn traccar_panel(key: &'static str) {
    let panel = document()
        .get_element_by_id("traccar_panel")
        .expect("there is a traccar panel");

    sycamore::render_to(
        move || {
            let show_live = create_signal(true);

            create_effect(move || {
                if show_live.get() {
                    let address = format!("https://{HOSTNAME}/{key}/live");
                    spawn_local_scoped(read_live(address))
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
                summary {"Traccar"}
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
                }
            }
        },
        &panel,
    );
}

pub fn option_panel() {
    let panel = document()
        .get_element_by_id("option_panel")
        .expect("there is a option panel");

    sycamore::render_to(
        move || {
            let show_me = create_signal(false);

            create_effect(move || {
                if show_me.get() {
                    spawn_local_scoped(my_loc())
                }
            });

            let group_names = create_signal(LocalStorage::get(GROUP_NAMES_KEY).unwrap_or(true));
            let group_names_zoom =
                create_signal(LocalStorage::get(GROUP_NAMES_ZOOM_KEY).unwrap_or(14.0_f64));

            create_effect(move || {
                let enabled = group_names.get();
                let zoom = group_names_zoom.get();
                // an emptied number field is NaN
                if zoom.is_nan() {
                    return;
                }
                let _ = LocalStorage::set(GROUP_NAMES_KEY, enabled);
                let _ = LocalStorage::set(GROUP_NAMES_ZOOM_KEY, zoom);
                set_group_names(enabled, zoom);
            });

            view! {
                summary {"Opties"}
                div(class="field"){
                    label(r#for="mijn"){"Mijn locatie:"}
                    input(id="mijn", r#type="checkbox", bind:checked=show_me)
                }
                div(class="field"){
                    label(r#for="group_names"){"Groepsnamen tonen:"}
                    input(id="group_names", r#type="checkbox", bind:checked=group_names)
                }
                div(class="field"){
                    label(r#for="group_names_zoom"){"Vanaf zoomniveau:"}
                    input(
                        id="group_names_zoom",
                        r#type="number",
                        min="1",
                        max="19",
                        disabled=!group_names.get(),
                        bind:valueAsNumber=group_names_zoom,
                    )
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
