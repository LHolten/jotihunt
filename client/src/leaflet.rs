use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/leaflet.js")]
extern "C" {
    type JsMarker;

    fn add_marker(lat: f64, lng: f64, name: String, convert: bool) -> JsMarker;
    #[wasm_bindgen(js_name = remove_layer)]
    fn remove_marker(marker: &JsMarker);
    fn set_marker_color(marker: &JsMarker, color: &str);
    fn set_human(marker: &JsMarker);
    fn set_fox(marker: &JsMarker);

    fn zoom_to(marker: &JsMarker);

    type JsLine;

    fn new_line(fox: &str) -> JsLine;
    fn add_line_marker(line: &JsLine, marker: &JsMarker);
    #[wasm_bindgen(js_name = remove_layer)]
    fn remove_line(line: &JsLine);
}

pub struct Marker(JsMarker);

impl Marker {
    pub fn new(lat: f64, lng: f64, name: String, convert: bool) -> Self {
        Self(add_marker(lat, lng, name, convert))
    }
    pub fn set_color(&self, color: &str) {
        set_marker_color(&self.0, color)
    }
    pub fn set_human(&self) {
        set_human(&self.0)
    }
    pub fn set_fox(&self) {
        set_fox(&self.0)
    }
    pub fn zoom_to(&self) {
        zoom_to(&self.0)
    }
}

impl Drop for Marker {
    fn drop(&mut self) {
        remove_marker(&self.0)
    }
}

pub struct Line(JsLine);

impl Line {
    pub fn new(fox: &str) -> Self {
        Self(new_line(fox))
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
