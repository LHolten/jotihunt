use std::{sync::Arc, time::Duration};

use arc_swap::ArcSwap;
use serde::Deserialize;
use sled::Db;
use tokio::time::sleep;

use crate::leak;

#[derive(Deserialize)]
struct Areas {
    data: Vec<Area>,
}

#[derive(Deserialize)]
struct Area {
    name: String,
    status: String,
    updated_at: String,
}

fn update_single_status(tree: &sled::Tree, area: Area) -> Result<(), postcard::Error> {
    let key = postcard::to_allocvec(&(&area.updated_at, &area.name))?;
    let value = postcard::to_allocvec(&area.status)?;
    if tree.get(&key).unwrap().as_slice() != Some(&value).as_slice() {
        let _old = tree.insert(&key, value).unwrap();
    }
    Ok(())
}

async fn retrieve_status_inner(tree: &sled::Tree) -> Result<String, reqwest::Error> {
    let res = reqwest::get("https://jotihunt.nl/api/2.0/areas").await?;
    let areas: Areas = res.json().await?;
    let mut foxes = vec![];
    for area in areas.data {
        foxes.push(area.name.clone());
        if let Err(err) = update_single_status(tree, area) {
            println!("error handling area: {err}")
        }
    }
    Ok(serde_json::to_string(&foxes).unwrap())
}

pub async fn retrieve_status_loop(db: &Db) -> &'static ArcSwap<String> {
    let tree = db.open_tree("status").unwrap();
    let list = retrieve_status_inner(&tree).await.unwrap();
    let arc = leak(ArcSwap::new(Arc::new(list)));

    tokio::spawn(async move {
        loop {
            // every minute
            sleep(Duration::from_secs(60)).await;

            println!("reloading status");
            match retrieve_status_inner(&tree).await {
                Ok(list) => {
                    arc.store(Arc::new(list));
                }
                Err(err) => {
                    println!("error getting status: {err}");
                }
            }
        }
    });

    arc
}
