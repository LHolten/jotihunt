mod article;
mod db;
mod geojson;
mod location;
mod login;
mod record;
mod status;
mod traccar;

use std::{
    collections::HashMap,
    convert::Infallible,
    fs::{File, read_to_string},
    io::Write,
    sync::Arc,
};

use anyhow::Context;
use arc_swap::ArcSwap;
use article::{article_content, retrieve_articles_loop};
use async_stream::stream;
use axum::{
    Router,
    body::Bytes,
    extract::{Path, Request},
    http::StatusCode,
    middleware::Next,
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{any, get, post},
};
use clap::Parser;
use futures_util::{Stream, StreamExt};
use geojson::{get_geo, reload_geojson_loop};
use jotihunt_shared::{
    Broadcast, Traccar,
    domain::{ArticleKey, FoxKey},
};
use location::{create_location, delete_location, modify_location};
use login::login;
use record::Recorder;
use rust_query::{
    Database, DatabaseAsync, Expr, FromExpr, Select, Table, TableRow, migration::Config,
    private::IntoJoinable,
};
use serde::Serialize;

use db::{Article, ArticleRow, Location, LocationRow, Schema, Status, StatusRow};
use status::{retrieve_status, retrieve_status_loop};
use tokio::sync::broadcast::{self, error::RecvError};
use tower_http::cors::CorsLayer;
use traccar::{live_events, receive_location};
use uuid::Uuid;

#[derive(Parser)]
struct Args {
    /// Save the responses of the jotihunt.nl API in this directory
    #[arg(long)]
    record: Option<std::path::PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    if let Ok(mut file) = File::create_new("password") {
        write!(&mut file, "test").unwrap();
    }
    let password = read_to_string("password").unwrap();

    println!("password is: {}", password);

    let state = leak(AppState {
        record: args.record.map(Recorder::new),
        ..AppState::new(Config::open("joti.sqlite"))
    });
    state.geojson.store(Arc::new(get_geo(state).await.unwrap()));
    retrieve_status(state).await.unwrap();

    tokio::spawn(reload_geojson_loop(state));
    tokio::spawn(retrieve_status_loop(state));
    tokio::spawn(retrieve_articles_loop(state));

    let password = leak(password);
    let router = Router::new()
        .route("/login", post(move |form| login(state, password, form)))
        .nest(
            "/{key}",
            Router::new()
                // lets the client check if its stored secret is still valid
                .route("/check", get(|| async {}))
                .route(
                    "/locations",
                    get(move || {
                        event_stream(state, &state.live_locations, Location, |l| {
                            LocationRow::from_expr(l).map(|row| {
                                let (key, fox) = row.into_parts();
                                encode_broadcast(&key, Some(&fox))
                            })
                        })
                    }),
                )
                .route(
                    "/locations/{day}/{time}/{fox_name}",
                    post(move |Path(key): Path<FoxKey>, body: Bytes| async move {
                        match serde_json::from_slice(&body) {
                            Ok(fox) => create_location(state, key, fox).await,
                            Err(_) => StatusCode::BAD_REQUEST,
                        }
                    })
                    .put(move |Path(key): Path<FoxKey>, body: Bytes| async move {
                        match serde_json::from_slice(&body) {
                            Ok(edit) => modify_location(state, key, edit).await,
                            Err(_) => StatusCode::BAD_REQUEST,
                        }
                    })
                    .delete(
                        move |Path(key): Path<FoxKey>, body: Bytes| async move {
                            match serde_json::from_slice(&body) {
                                Ok(old) => delete_location(state, key, old).await,
                                Err(_) => StatusCode::BAD_REQUEST,
                            }
                        },
                    ),
                )
                .route(
                    "/live",
                    get(move || async move { live_events(state.live_traccar.subscribe()) }),
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
            any(move |method, uri, headers, body| {
                receive_location(state, method, uri, headers, body)
            }),
        )
        // the status and articles are public, so they are not behind the secret
        .route(
            "/status",
            get(move || {
                event_stream(state, &state.live_status, Status, |s| {
                    StatusRow::from_expr(s).map(|row| {
                        let (key, status) = row.into_parts();
                        encode_broadcast(&key, Some(&status))
                    })
                })
            }),
        )
        .route(
            "/articles",
            get(move || {
                event_stream(state, &state.live_articles, Article, |a| {
                    ArticleRow::from_expr(a).map(|row| {
                        let (key, article) = row.into_parts();
                        encode_broadcast(&key, Some(&article))
                    })
                })
            }),
        )
        .route(
            "/articles/{publish_at}/{id}",
            get(move |Path(key): Path<ArticleKey>| async move {
                article_content(state, key)
                    .await
                    .ok_or(StatusCode::NOT_FOUND)
            }),
        )
        .route(
            "/deelnemers.geojson",
            get(move || async move { state.geojson.load().as_ref().clone() }),
        )
        .route(
            "/areas.json",
            get(move || async move { state.areas.load().as_ref().clone() }),
        )
        // the client is served from a different origin
        .layer(CorsLayer::very_permissive());

    // release builds listen on a unix socket for the reverse proxy
    #[cfg(unix)]
    if !cfg!(debug_assertions) {
        use std::{
            fs::{Permissions, set_permissions},
            os::unix::fs::PermissionsExt,
        };

        let path = "/run/jotihunt/socket";
        let listener = tokio::net::UnixListener::bind(path)?;
        set_permissions(path, Permissions::from_mode(0o777))?;
        axum::serve(listener, router).await?;
        return Ok(());
    }

    // debug builds put caddy in front of a TCP port for local testing with `trunk serve`,
    // this is also what platforms without unix sockets do
    let addr = "127.0.0.1:3001";
    let listener = tokio::net::TcpListener::bind(addr).await?;
    std::process::Command::new("caddy")
        .args([
            "reverse-proxy",
            "--from",
            "https://localhost:3000",
            "--disable-redirects",
        ])
        .args(["--to", addr])
        .spawn()
        .context("failed to start caddy, is it installed?")?;
    println!("listening on https://localhost:3000");
    axum::serve(listener, router).await?;

    Ok(())
}

pub struct AppState {
    /// Needed in the url of all endpoints that access the database.
    pub secret: Uuid,
    /// Saves the responses of the jotihunt.nl API if set.
    pub record: Option<Recorder>,
    /// Shared so connections to the jotihunt.nl API are reused.
    pub http: reqwest::Client,
    pub db: DatabaseAsync<Schema>,
    pub live_traccar: broadcast::Sender<Traccar>,
    pub live_locations: Live,
    pub live_status: Live,
    pub live_articles: Live,
    pub geojson: ArcSwap<String>,
    /// JSON of a list of [jotihunt_shared::domain::Area].
    pub areas: ArcSwap<String>,
}

impl AppState {
    /// Nothing is recorded, `geojson` and `areas` start out empty.
    pub fn new(config: Config) -> Self {
        let b = reqwest::Client::builder();

        #[cfg(test)]
        // Tests don't access the network, and the build sandbox has no system certificates to load.
        let b = b.tls_certs_only([]);

        let db = Database::<Schema>::new(config);
        Self {
            secret: Uuid::new_v4(),
            record: None,
            http: b.build().unwrap(),
            db: DatabaseAsync::new(Arc::new(db)),
            live_traccar: broadcast::channel(16).0,
            live_locations: broadcast::channel(4096).0,
            live_status: broadcast::channel(4096).0,
            live_articles: broadcast::channel(4096).0,
            geojson: ArcSwap::default(),
            areas: ArcSwap::default(),
        }
    }
}

/// Every change to a table is sent on its [Live] channel as the JSON event data.
///
/// Changes are sent inside the transaction that makes them and clients subscribe
/// inside the (mutable) transaction that reads the initial data.
/// Mutable transactions are serialized, so clients never miss or reorder a change.
pub type Live = broadcast::Sender<String>;

/// Encode a change for the client, `None` means that the key was removed.
pub fn encode_broadcast<K: Serialize, V: Serialize>(key: &K, value: Option<&V>) -> String {
    serde_json::to_string(&Broadcast::Update { key, value }).unwrap()
}

/// Send all rows of a table and then all its changes to the client.
async fn event_stream<T: Table<Schema = Schema>>(
    state: &'static AppState,
    live: &'static Live,
    table: impl 'static + Send + for<'x> IntoJoinable<'x, Schema, Typ = TableRow<T>>,
    select: impl 'static
    + Send
    + for<'x> FnOnce(Expr<'x, Schema, TableRow<T>>) -> Select<'x, Schema, String>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
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

    let events = stream! {
        yield serde_json::to_string(&Broadcast::<(), ()>::Reset).unwrap();
        for data in snapshot {
            yield data;
        }
        loop {
            match changes.recv().await {
                Ok(data) => yield data,
                // Ending the stream makes the client reconnect,
                // it then gets a reset and all rows again.
                Err(RecvError::Lagged(_)) | Err(RecvError::Closed) => break,
            }
        }
    };
    Sse::new(events.map(|data| Ok(Event::default().data(data)))).keep_alive(KeepAlive::default())
}

fn leak<T>(val: T) -> &'static T {
    &*Box::leak(Box::new(val))
}

#[cfg(test)]
mod tests {
    use jotihunt_shared::domain::Fox;

    use super::*;

    #[tokio::test]
    async fn location_events() {
        let state = leak(AppState::new(Config::open_in_memory()));
        let key = |fox_name: &str| FoxKey {
            day: "2026-10-05".to_owned(),
            time: "10:00".to_owned(),
            fox_name: fox_name.to_owned(),
        };
        let fox = Fox {
            latitude: "1".to_owned(),
            longitude: "2".to_owned(),
        };
        location::create_location(state, key("Alpha"), fox.clone()).await;

        let sse = event_stream(state, &state.live_locations, Location, |l| {
            LocationRow::from_expr(l).map(|row| {
                let (key, fox) = row.into_parts();
                encode_broadcast(&key, Some(&fox))
            })
        })
        .await;
        location::create_location(state, key("Bravo"), fox.clone()).await;

        let mut body = sse.into_response().into_body().into_data_stream();
        let mut text = String::new();
        while text.matches("\n\n").count() < 3 {
            text += std::str::from_utf8(&body.next().await.unwrap().unwrap()).unwrap();
        }
        let alpha = r#"{"key":{"day":"2026-10-05","time":"10:00","fox_name":"Alpha"},"value":{"latitude":"1","longitude":"2"}}"#;
        let bravo = alpha.replace("Alpha", "Bravo");
        assert_eq!(
            text,
            format!(
                "data: \"Reset\"\n\ndata: {{\"Update\":{alpha}}}\n\ndata: {{\"Update\":{bravo}}}\n\n"
            )
        );
    }
}
