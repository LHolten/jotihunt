use std::ops::{Deref, Not};

use anyhow::Context;
use async_stream::stream;
use futures_util::{future, pin_mut, StreamExt, TryStreamExt};
use jotihunt_client::update::{AtomicEdit, Broadcast};
use sled::{Db, Event};
use tokio::{
    net::{TcpListener, TcpStream},
    runtime::Runtime,
};
use tokio_tungstenite::tungstenite::Message;

fn main() {
    Runtime::new().unwrap().block_on(async {
        let db = Box::leak(Box::new(sled::open("joti.db").unwrap()));
        println!("{} items in db", db.scan_prefix([]).count());

        // Create the event loop and TCP listener we'll accept connections on.
        let listener = TcpListener::bind(&"127.0.0.1:8090").await.unwrap();

        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(accept_and_log(stream, db));
        }
    })
}

async fn accept_and_log(stream: TcpStream, db: &Db) {
    match accept_connection(stream, db).await {
        Ok(()) => {}
        Err(e) => {
            println!("error on connection: {}", e)
        }
    }
}

async fn accept_connection(stream: TcpStream, db: &Db) -> anyhow::Result<()> {
    let ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .context("Error during the websocket handshake occurred")?;
    println!("client connected");

    let (write, read) = ws_stream.split();
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
