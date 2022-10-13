use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/leaflet.js")]
extern "C" {
    type JsMarker;

    fn add_marker(lat: f64, lng: f64, name: String, convert: bool) -> JsMarker;
    #[wasm_bindgen(js_name = remove_layer)]
    fn remove_marker(marker: &JsMarker);
    fn set_marker_color(marker: &JsMarker, last: bool);

    type JsLine;

    fn new_line() -> JsLine;
    fn add_line_marker(line: &JsLine, marker: &JsMarker);
    #[wasm_bindgen(js_name = remove_layer)]
    fn remove_line(line: &JsLine);
}

pub struct Marker(JsMarker);

impl Marker {
    pub fn new(lat: f64, lng: f64, name: String, convert: bool) -> Self {
        Self(add_marker(lat, lng, name, convert))
    }
    pub fn set_color(&self, last: bool) {
        set_marker_color(&self.0, last)
    }
}

impl Drop for Marker {
    fn drop(&mut self) {
        remove_marker(&self.0)
    }
}

pub struct Line(JsLine);

impl Line {
    pub fn new() -> Self {
        Self(new_line())
    }
    pub fn push(&self, marker: &Marker) {
        add_line_marker(&self.0, &marker.0)
    }
}

impl Drop for Line {
    fn drop(&mut self) {
        remove_line(&self.0)
    }
}
