use std::{
    env,
    ops::{Deref, Not},
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
use futures_util::{future, pin_mut, StreamExt, TryStreamExt};
use hyper::StatusCode;
use jotihunt_client::update::{AtomicEdit, Broadcast};
use sled::{Db, Event};

use tower::{make::Shared, ServiceBuilder};
use tower_http::{auth::RequireAuthorizationLayer, cors::CorsLayer};
use uuid::Uuid;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let addr: std::net::SocketAddr = "0.0.0.0:8090".parse()?;

    let args: Vec<String> = env::args().collect();
    let password = &*Box::leak(Box::new(args[1].clone()));
    println!("password is: {password}");
    let secret = Uuid::new_v4();

    let db = &*Box::leak(Box::new(sled::open("joti.db").unwrap()));
    println!("{} items in db", db.scan_prefix([]).count());

    let router = Router::new()
        .route(
            "/secret",
            ServiceBuilder::new()
                .layer(CorsLayer::very_permissive().allow_credentials(true))
                .layer(RequireAuthorizationLayer::basic("", password))
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

    hyper::Server::bind(&addr)
        .serve(Shared::new(router))
        .await?;
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
