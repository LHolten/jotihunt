use std::{
    net::SocketAddr,
    ops::{Deref, Not},
    time::Duration,
};

use async_stream::stream;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, WebSocketUpgrade,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use axum_server::tls_rustls::RustlsConfig;
use clap::Parser;
use futures_util::{future, pin_mut, StreamExt, TryStreamExt};
use hyper::StatusCode;
use jotihunt_client::update::{AtomicEdit, Broadcast};
use sled::{Db, Event};

use tokio::time::sleep;
use tower::{make::Shared, ServiceBuilder};
use tower_http::{auth::RequireAuthorizationLayer, cors::CorsLayer};
use uuid::Uuid;

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
    let args = &*Box::leak(Box::new(Args::parse()));

    println!("password is: {}", args.password);
    let secret = Uuid::new_v4();

    let db = &*Box::leak(Box::new(sled::open("joti.db").unwrap()));
    println!("{} items in db", db.scan_prefix([]).count());

    let router = Router::new()
        .route(
            "/secret",
            ServiceBuilder::new()
                .layer(CorsLayer::very_permissive().allow_credentials(true))
                .layer(RequireAuthorizationLayer::basic("", &args.password))
                .service(get(move || async move { secret.to_string() })),
        )
        .route(
            "/:key",
            ServiceBuilder::new().service(get(
                move |req: WebSocketUpgrade, Path(key): Path<Uuid>| async move {
                    if key != secret {
                        return StatusCode::UNAUTHORIZED.into_response();
                    }
                    req.on_upgrade(|ws| accept_and_log(ws, db))
                },
            )),
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
