use std::mem::forget;

use gloo::{events::EventListener, net::websocket::futures::WebSocket, utils::document};
use mk_geolocation::callback::Position;
use wasm_bindgen::prelude::*;

mod update;

#[wasm_bindgen(module = "/leaflet.js")]
extern "C" {
    type Map;

    fn make_map() -> Map;

    type Marker;

    fn add_marker(map: &Map, lat: f64, lng: f64) -> Marker;

}

fn main() {
    let map = &*Box::leak(Box::new(make_map()));

    let pos = Position::new(move |p| {
        add_marker(map, p.coords().latitude(), p.coords().longitude());
    });
    forget(pos);

    let button = document()
        .get_element_by_id("add_point")
        .expect("there is a add_point button");

    let on_click = EventListener::new(&button, "click", move |_event| {
        add_marker(map, 51.5, -0.09);
    });
    on_click.forget();

    let mut ws = WebSocket::open("wss://echo.websocket.org").unwrap();
    // let (mut write, mut read) = ws.split();
}
