mod geojson;

use std::{
    net::SocketAddr,
    ops::{Deref, Not},
    sync::Arc,
    time::Duration,
};

use arc_swap::ArcSwap;
use async_stream::stream;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, Query, WebSocketUpgrade,
    },
    response::IntoResponse,
    routing::{any, get},
    Router,
};
use axum_server::tls_rustls::RustlsConfig;
use clap::Parser;
use futures_util::{future, pin_mut, StreamExt, TryStreamExt};
use hyper::StatusCode;
use jotihunt_shared::{AtomicEdit, Broadcast, Traccar};
use sled::{Db, Event};

use tokio::{
    sync::broadcast::{self, error::RecvError},
    time::sleep,
};
use tower::{make::Shared, ServiceBuilder};
use tower_http::{auth::RequireAuthorizationLayer, cors::CorsLayer};
use uuid::Uuid;

use crate::geojson::get_geo;

#[derive(Parser)]
struct Args {
    /// Name of the server certificate to load for TLS
    #[arg(short, long)]
    domain: Option<String>,

    /// The password to use for the authentication
    #[arg(short, long)]
    password: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = leak(Args::parse());

    println!("password is: {}", args.password);
    let secret = Uuid::new_v4();

    let db = leak(sled::open("joti.db").unwrap());
    println!("{} items in db", db.scan_prefix([]).count());

    let live = leak(broadcast::channel(16).0);

    let geojson = get_geo().await.unwrap();
    let geojson = Arc::new(ArcSwap::new(Arc::new(geojson)));
    tokio::spawn(reload_geojson(geojson.clone()));

    let router = Router::new()
        .route(
            "/secret",
            ServiceBuilder::new()
                .layer(CorsLayer::very_permissive())
                .layer(RequireAuthorizationLayer::basic("", &args.password))
                .service(get(move || async move { secret.to_string() })),
        )
        .route(
            "/:key",
            get(
                move |req: WebSocketUpgrade, Path(key): Path<Uuid>| async move {
                    if key != secret {
                        return StatusCode::UNAUTHORIZED.into_response();
                    }
                    req.on_upgrade(|ws| accept_and_log(ws, db))
                },
            ),
        )
        .route(
            "/traccar",
            any(|traccar: Query<Traccar>| async {
                let _ = live.send(traccar.0);
                StatusCode::OK.into_response()
            }),
        )
        .route(
            "/live/:key",
            get(
                move |req: WebSocketUpgrade, Path(key): Path<Uuid>| async move {
                    if key != secret {
                        return StatusCode::UNAUTHORIZED.into_response();
                    }
                    req.on_upgrade(|ws| live_ws(ws, live.subscribe()))
                },
            ),
        )
        .route(
            "/deelnemers.geojson",
            ServiceBuilder::new()
                .layer(CorsLayer::very_permissive())
                .service(get(move || async move { geojson.load().as_ref().clone() })),
        );

    let addr = SocketAddr::from(([0, 0, 0, 0], 4848));

    if let Some(domain) = &args.domain {
        let config = RustlsConfig::from_pem_file(
            format!("/etc/letsencrypt/live/{domain}/fullchain.pem"),
            format!("/etc/letsencrypt/live/{domain}/privkey.pem"),
        )
        .await
        .unwrap();

        tokio::spawn(reload(config.clone(), domain));
        axum_server::bind_rustls(addr, config)
            .serve(Shared::new(router))
            .await?;
    } else {
        axum_server::bind(addr).serve(Shared::new(router)).await?;
    }

    Ok(())
}

async fn accept_and_log(stream: WebSocket, db: &Db) {
    match accept_connection(stream, db).await {
        Ok(()) => {}
        Err(e) => {
            println!("error on connection: {}", e)
        }
    }
}

async fn accept_connection(stream: WebSocket, db: &Db) -> anyhow::Result<()> {
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
            println!("received(bin): {:?}", bin);
            println!("received: {:?}, {:?}, {:?}", edit.key, old, new);

            let _ = db.compare_and_swap(edit.key, old, new).unwrap();
            Ok(())
        });

    let send_edits = stream! {
        let mut subscriber = db.watch_prefix([]);
        for pair in db.deref() {
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
        println!("sending: {:?}", bin);
        Ok(Message::Binary(bin))
    })
    .forward(write);

    pin_mut!(receive_edits, send_edits);
    future::select(receive_edits, send_edits).await;

    Ok(())
}

async fn reload(config: RustlsConfig, domain: &str) {
    loop {
        sleep(Duration::from_secs(100_000)).await;
        println!("reloading rustls configuration");

        config
            .reload_from_pem_file(
                format!("/etc/letsencrypt/live/{domain}/fullchain.pem"),
                format!("/etc/letsencrypt/live/{domain}/privkey.pem"),
            )
            .await
            .unwrap();
    }
}

type LiveReceiver = broadcast::Receiver<Traccar>;

async fn live_ws(mut stream: WebSocket, mut live: LiveReceiver) {
    loop {
        match live.recv().await {
            Ok(traccar) => {
                let bin = postcard::to_stdvec(&traccar).unwrap();
                let Ok(()) = stream.send(Message::Binary(bin)).await else {
                    break;
                };
            }
            Err(RecvError::Closed) => break,
            Err(RecvError::Lagged(_)) => continue,
        }
    }
}

async fn reload_geojson(geo: Arc<ArcSwap<String>>) {
    loop {
        // every hour
        sleep(Duration::from_secs(60 * 60)).await;
        println!("reloading geojson");
        match get_geo().await {
            Ok(new) => geo.swap(Arc::new(new)),
            Err(err) => {
                println!("error getting geojson: {err}");
                continue;
            }
        };
    }
}

fn leak<T>(val: T) -> &'static T {
    &*Box::leak(Box::new(val))
}
