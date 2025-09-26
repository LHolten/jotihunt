mod article;
mod geojson;
mod status;

use std::{
    fs::{read_to_string, File},
    io::Write,
    ops::Not,
};

use article::retrieve_articles_loop;
use async_stream::stream;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, Query, Request, WebSocketUpgrade,
    },
    http::StatusCode,
    middleware::Next,
    response::IntoResponse,
    routing::{any, get},
    Router,
};
use futures_util::{future, pin_mut, StreamExt, TryStreamExt};
use geojson::get_reloading_geojson;
use jotihunt_shared::{AtomicEdit, Broadcast, Traccar};
use sled::{Event, Tree};

use status::retrieve_status_loop;
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
    let secret = Uuid::new_v4();

    let db = leak(sled::open("joti.db").unwrap());
    println!("{} items in db", db.scan_prefix([]).count());

    let live = leak(broadcast::channel(16).0);

    let geojson = get_reloading_geojson().await;
    let fox_list = retrieve_status_loop(db).await;
    tokio::spawn(retrieve_articles_loop(db));

    let router = Router::new()
        .route(
            "/secret",
            get(move || async move { secret.to_string() })
                .route_layer(CorsLayer::very_permissive())
                .layer(ValidateRequestHeaderLayer::basic("", &password)),
        )
        .nest(
            "/{key}",
            Router::new()
                .route(
                    "/locations",
                    get(move |req: WebSocketUpgrade| async move {
                        req.on_upgrade(|ws| accept_and_log(ws, db))
                    }),
                )
                .route(
                    "/status",
                    get(move |req: WebSocketUpgrade| async move {
                        req.on_upgrade(|ws| async {
                            let tree = db.open_tree("status").unwrap();
                            accept_and_log(ws, &tree).await
                        })
                    }),
                )
                .route(
                    "/articles",
                    get(move |req: WebSocketUpgrade| async move {
                        req.on_upgrade(|ws| async {
                            let tree = db.open_tree("articles").unwrap();
                            accept_and_log(ws, &tree).await
                        })
                    }),
                )
                .route(
                    "/live",
                    get(move |req: WebSocketUpgrade| async move {
                        req.on_upgrade(|ws| live_ws(ws, live.subscribe()))
                    }),
                )
                .route_layer(axum::middleware::from_fn(
                    move |Path(key): Path<Uuid>, request: Request, next: Next| async move {
                        if key != secret {
                            return StatusCode::UNAUTHORIZED.into_response();
                        }
                        next.run(request).await
                    },
                )),
        )
        .route(
            "/traccar",
            any(|traccar: Query<Traccar>| async {
                let _ = live.send(traccar.0);
                StatusCode::OK.into_response()
            }),
        )
        .route(
            "/deelnemers.geojson",
            get(move || async move { geojson.load().as_ref().clone() })
                .route_layer(CorsLayer::very_permissive()),
        )
        .route(
            "/fox_list.json",
            get(move || async move { fox_list.load().as_ref().clone() })
                .route_layer(CorsLayer::very_permissive()),
        );

    let listener = tokio::net::UnixListener::bind("/run/jotihunt.socket")?;
    axum::serve(listener, router).await?;

    Ok(())
}

async fn accept_and_log(stream: WebSocket, db: &Tree) {
    match accept_connection(stream, db).await {
        Ok(()) => {}
        Err(e) => {
            println!("error on connection: {}", e)
        }
    }
}

async fn accept_connection(stream: WebSocket, db: &Tree) -> anyhow::Result<()> {
    println!("client connected");

    let (write, read) = stream.split();
    let receive_edits = read
        .map_err(anyhow::Error::from)
        .try_filter_map(|msg| match msg {
            Message::Binary(b) => future::ok(Some(b)),
            _ => future::ok(None),
        })
        .try_for_each(|bin| async move {
            let edit: AtomicEdit = postcard::from_bytes(&bin)?;
            let new = edit.new.is_empty().not().then_some(edit.new);
            let old = edit.old.is_empty().not().then_some(edit.old);
            // println!("received(bin): {:?}", bin);
            // println!("received: {:?}, {:?}, {:?}", edit.key, old, new);

            let _ = db.compare_and_swap(edit.key, old, new).unwrap();
            Ok(())
        });

    let send_edits = stream! {
        let mut subscriber = db.watch_prefix([]);
        for pair in db {
            yield pair.unwrap();
        }
        while let Some(event) = (&mut subscriber).await {
            match event {
                Event::Insert { key, value } => {
                    yield (key, value)
                }
                Event::Remove { key } => {
                    yield (key, Default::default())
                }
            }
        }
    }
    .map(|(key, value)| {
        let bin = postcard::to_stdvec(&Broadcast {
            key: key.as_ref().to_owned(),
            value: value.as_ref().to_owned(),
        })
        .unwrap();
        // println!("sending: {:?}", bin);
        Ok(Message::Binary(axum::body::Bytes::from_owner(bin)))
    })
    .forward(write);

    pin_mut!(receive_edits, send_edits);
    future::select(receive_edits, send_edits).await;

    println!("client disconnected");

    Ok(())
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
