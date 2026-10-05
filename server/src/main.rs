mod article;
mod db;
mod geojson;
mod location;
mod status;

use std::{
    collections::HashMap,
    fs::{read_to_string, set_permissions, File, Permissions},
    io::Write,
    os::unix::fs::PermissionsExt,
    sync::Arc,
};

use arc_swap::ArcSwap;
use article::retrieve_articles_loop;
use async_stream::stream;
use axum::{
    body::Bytes,
    extract::{
        ws::{Message, WebSocket},
        Json, Path, Query, Request, WebSocketUpgrade,
    },
    http::StatusCode,
    middleware::Next,
    response::IntoResponse,
    routing::{any, get, post},
    RequestExt, Router,
};
use futures_util::{future, pin_mut, StreamExt};
use geojson::{get_geo, reload_geojson_loop};
use jotihunt_shared::{domain::FoxKey, Broadcast, Traccar};
use location::{create_location, delete_location, modify_location};
use rust_query::{
    migration::Config, private::IntoJoinable, Database, DatabaseAsync, Expr, FromExpr, Select,
    Table, TableRow,
};
use serde::Serialize;

use db::{Article, ArticleRow, Location, LocationRow, Schema, Status, StatusRow};
use status::{retrieve_status, retrieve_status_loop};
use tokio::sync::broadcast::{self, error::RecvError};
use tower_http::{cors::CorsLayer, validate_request::ValidateRequestHeaderLayer};
use uuid::Uuid;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if let Ok(mut file) = File::create_new("password") {
        write!(&mut file, "test").unwrap();
    }
    let password = read_to_string("password").unwrap();

    println!("password is: {}", password);

    let state = leak(AppState::new(Config::open("joti.sqlite")));
    state.geojson.store(Arc::new(get_geo().await.unwrap()));
    retrieve_status(state).await.unwrap();

    tokio::spawn(reload_geojson_loop(state));
    tokio::spawn(retrieve_status_loop(state));
    tokio::spawn(retrieve_articles_loop(state));

    let router = Router::new()
        .route(
            "/secret",
            get(move || async move { state.secret.to_string() })
                .route_layer(CorsLayer::very_permissive())
                .layer(ValidateRequestHeaderLayer::basic("", &password)),
        )
        .nest(
            "/{key}",
            Router::new()
                .route(
                    "/locations",
                    get(move |req: WebSocketUpgrade| async move {
                        req.on_upgrade(move |ws| {
                            accept_and_log(ws, state, &state.live_locations, Location, |l| {
                                LocationRow::from_expr(l).map(|row| {
                                    let (key, fox) = row.into_parts();
                                    encode_broadcast(&key, Some(&fox))
                                })
                            })
                        })
                    }),
                )
                .route(
                    "/locations/{day}/{time}/{fox_name}",
                    post(move |Path(key): Path<FoxKey>, body: Bytes| async move {
                        match postcard::from_bytes(&body) {
                            Ok(fox) => create_location(state, key, fox).await,
                            Err(_) => StatusCode::BAD_REQUEST,
                        }
                    })
                    .put(move |Path(key): Path<FoxKey>, body: Bytes| async move {
                        match postcard::from_bytes(&body) {
                            Ok(edit) => modify_location(state, key, edit).await,
                            Err(_) => StatusCode::BAD_REQUEST,
                        }
                    })
                    .delete(move |Path(key): Path<FoxKey>, body: Bytes| async move {
                        match postcard::from_bytes(&body) {
                            Ok(old) => delete_location(state, key, old).await,
                            Err(_) => StatusCode::BAD_REQUEST,
                        }
                    })
                    .layer(CorsLayer::very_permissive()),
                )
                .route(
                    "/status",
                    get(move |req: WebSocketUpgrade| async move {
                        req.on_upgrade(move |ws| {
                            accept_and_log(ws, state, &state.live_status, Status, |s| {
                                StatusRow::from_expr(s).map(|row| {
                                    let (key, status) = row.into_parts();
                                    encode_broadcast(&key, Some(&status))
                                })
                            })
                        })
                    }),
                )
                .route(
                    "/articles",
                    get(move |req: WebSocketUpgrade| async move {
                        req.on_upgrade(move |ws| {
                            accept_and_log(ws, state, &state.live_articles, Article, |a| {
                                ArticleRow::from_expr(a).map(|row| {
                                    let (key, article) = row.into_parts();
                                    encode_broadcast(&key, Some(&article))
                                })
                            })
                        })
                    }),
                )
                .route(
                    "/live",
                    get(move |req: WebSocketUpgrade| async move {
                        req.on_upgrade(|ws| live_ws(ws, state.live_traccar.subscribe()))
                    }),
                )
                .route_layer(axum::middleware::from_fn(
                    move |Path(params): Path<HashMap<String, String>>,
                          request: Request,
                          next: Next| async move {
                        // nested routes have more path parameters than just the key
                        if params.get("key").and_then(|key| key.parse().ok()) != Some(state.secret)
                        {
                            return StatusCode::UNAUTHORIZED.into_response();
                        }
                        next.run(request).await
                    },
                )),
        )
        .route(
            "/traccar",
            any(move |mut req: Request| async move {
                let traccar = match req.extract_parts::<Query<Traccar>>().await {
                    Ok(t) => t.0,
                    Err(_e) => {
                        let json: Json<NewTraccar> = match req.extract().await {
                            Ok(v) => v,
                            Err(e) => return e.into_response(),
                        };
                        Traccar {
                            id: json.0.device_id,
                            lat: json.0.location.coords.latitude.to_string(),
                            lon: json.0.location.coords.longitude.to_string(),
                        }
                    }
                };
                let _ = state.live_traccar.send(traccar);
                StatusCode::OK.into_response()
            }),
        )
        .route(
            "/deelnemers.geojson",
            get(move || async move { state.geojson.load().as_ref().clone() })
                .route_layer(CorsLayer::very_permissive()),
        )
        .route(
            "/fox_list.json",
            get(move || async move { state.fox_list.load().as_ref().clone() })
                .route_layer(CorsLayer::very_permissive()),
        );

    let path = "/run/jotihunt/socket";
    let listener = tokio::net::UnixListener::bind(path)?;
    set_permissions(path, Permissions::from_mode(0o777))?;
    axum::serve(listener, router).await?;

    Ok(())
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

pub struct AppState {
    /// Needed in the url of all endpoints that access the database.
    pub secret: Uuid,
    pub db: DatabaseAsync<Schema>,
    pub live_traccar: broadcast::Sender<Traccar>,
    pub live_locations: Live,
    pub live_status: Live,
    pub live_articles: Live,
    pub geojson: ArcSwap<String>,
    pub fox_list: ArcSwap<String>,
}

impl AppState {
    /// `geojson` and `fox_list` start out empty.
    pub fn new(config: Config) -> Self {
        let db = Database::<Schema>::new(config);
        Self {
            secret: Uuid::new_v4(),
            db: DatabaseAsync::new(Arc::new(db)),
            live_traccar: broadcast::channel(16).0,
            live_locations: broadcast::channel(4096).0,
            live_status: broadcast::channel(4096).0,
            live_articles: broadcast::channel(4096).0,
            geojson: ArcSwap::default(),
            fox_list: ArcSwap::default(),
        }
    }
}

/// Every change to a table is sent on its [Live] channel.
///
/// Changes are sent inside the transaction that makes them and clients subscribe
/// inside the (mutable) transaction that reads the initial data.
/// Mutable transactions are serialized, so clients never miss or reorder a change.
pub type Live = broadcast::Sender<Bytes>;

/// Encode a change for the client, `None` means that the key was removed.
pub fn encode_broadcast<K: Serialize, V: Serialize>(key: &K, value: Option<&V>) -> Bytes {
    let broadcast = Broadcast {
        key: postcard::to_stdvec(key).unwrap(),
        value: value.map_or(vec![], |v| postcard::to_stdvec(v).unwrap()),
    };
    Bytes::from_owner(postcard::to_stdvec(&broadcast).unwrap())
}

/// Send all rows of a table and then all its changes to the client.
async fn accept_and_log<T: Table<Schema = Schema>>(
    stream: WebSocket,
    state: &'static AppState,
    live: &'static Live,
    table: impl 'static + Send + for<'x> IntoJoinable<'x, Schema, Typ = TableRow<T>>,
    select: impl 'static
        + Send
        + for<'x> FnOnce(Expr<'x, Schema, TableRow<T>>) -> Select<'x, Schema, Bytes>,
) {
    println!("client connected");

    // This uses a mutable transaction, because those are serialized with all
    // transactions that send changes. That is what guarantees no change is missed.
    let (snapshot, mut changes) = state
        .db
        .transaction_mut_ok(move |txn| {
            let changes = live.subscribe();
            let snapshot = txn.query(|rows| {
                let row = rows.join(table);
                rows.into_vec(select(row))
            });
            (snapshot, changes)
        })
        .await;

    let (write, read) = stream.split();
    // The client doesn't send anything, but reading notices when it disconnects.
    let receive = read.for_each(|_| future::ready(()));

    let send = stream! {
        for bin in snapshot {
            yield bin;
        }
        loop {
            match changes.recv().await {
                Ok(bin) => yield bin,
                // The client can not recover from missed changes,
                // so we disconnect and let it reload the page.
                Err(RecvError::Lagged(_)) | Err(RecvError::Closed) => break,
            }
        }
    }
    .map(|bin| Ok(Message::Binary(bin)))
    .forward(write);

    pin_mut!(receive, send);
    if let future::Either::Right((Err(e), _)) = future::select(receive, send).await {
        println!("error on connection: {}", e)
    }

    println!("client disconnected");
}

type LiveReceiver = broadcast::Receiver<Traccar>;

async fn live_ws(mut stream: WebSocket, mut live: LiveReceiver) {
    loop {
        match live.recv().await {
            Ok(traccar) => {
                let bin = postcard::to_stdvec(&traccar).unwrap();
                let Ok(()) = stream
                    .send(Message::Binary(axum::body::Bytes::from_owner(bin)))
                    .await
                else {
                    break;
                };
            }
            Err(RecvError::Closed) => break,
            Err(RecvError::Lagged(_)) => continue,
        }
    }
}

fn leak<T>(val: T) -> &'static T {
    &*Box::leak(Box::new(val))
}
