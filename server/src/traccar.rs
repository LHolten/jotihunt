use std::convert::Infallible;

use async_stream::stream;
use axum::{
    body::Bytes,
    http::{header::CONTENT_TYPE, HeaderMap, Method, StatusCode, Uri},
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::{Stream, StreamExt};
use jotihunt_shared::Traccar;
use tokio::sync::broadcast::{self, error::RecvError};

use crate::AppState;

/// Sends a location from a traccar client to everyone watching the live locations.
pub async fn receive_location(
    state: &'static AppState,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    let Some(traccar) = parse_traccar(uri.query().unwrap_or_default(), &body) else {
        // logged to find out what format the client uses
        eprintln!(
            "unknown traccar request: {method} {uri} {:?} {}",
            headers.get(CONTENT_TYPE),
            String::from_utf8_lossy(&body)
        );
        return StatusCode::BAD_REQUEST;
    };
    let _ = state.live_traccar.send(traccar);
    StatusCode::OK
}

/// Reads a location in any format of the traccar clients, ignoring the content type.
fn parse_traccar(query: &str, body: &[u8]) -> Option<Traccar> {
    // the old client sends the location in the query, or as a form in the body
    let form = |data: &[u8]| serde_urlencoded::from_bytes::<Traccar>(data).ok();
    // the new client sends json
    let json = || {
        let json: NewTraccar = serde_json::from_slice(body).ok()?;
        Some(Traccar {
            id: json.device_id,
            lat: json.location.coords.latitude.to_string(),
            lon: json.location.coords.longitude.to_string(),
        })
    };
    form(query.as_bytes()).or_else(json).or_else(|| form(body))
}

#[derive(serde::Deserialize)]
struct NewTraccar {
    device_id: String,
    location: NewLocation,
}

#[derive(serde::Deserialize)]
struct NewLocation {
    coords: NewCoords,
}

#[derive(serde::Deserialize)]
struct NewCoords {
    latitude: f64,
    longitude: f64,
}

/// Send the locations from traccar as JSON event data.
pub fn live_events(
    mut live: broadcast::Receiver<Traccar>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let events = stream! {
        loop {
            match live.recv().await {
                Ok(traccar) => yield serde_json::to_string(&traccar).unwrap(),
                Err(RecvError::Closed) => break,
                // old locations are not important
                Err(RecvError::Lagged(_)) => continue,
            }
        }
    };
    Sse::new(events.map(|data| Ok(Event::default().data(data)))).keep_alive(KeepAlive::default())
}

#[test]
fn traccar_formats() {
    let id = |t: Option<Traccar>| t.map(|t| (t.id, t.lat, t.lon));
    let expected = Some(("phone".to_owned(), "52.1".to_owned(), "5.8".to_owned()));
    let form = b"id=phone&lat=52.1&lon=5.8&timestamp=1";
    assert_eq!(
        id(parse_traccar("id=phone&lat=52.1&lon=5.8", b"")),
        expected
    );
    assert_eq!(id(parse_traccar("", form)), expected);
    let json = br#"{"device_id":"phone","location":{"coords":{"latitude":52.1,"longitude":5.8}}}"#;
    assert_eq!(id(parse_traccar("", json)), expected);
    assert!(parse_traccar("", b"").is_none());
}
