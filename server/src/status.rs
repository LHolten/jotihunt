use std::time::Duration;

use serde::Deserialize;
use sled::Db;
use tokio::time::sleep;

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
    let prefix = postcard::to_allocvec(&area.name)?;
    if let Some(last_status) = tree.scan_prefix(prefix).last() {
        let (_, old_val) = last_status.unwrap();
        let old_status: String = postcard::from_bytes(&old_val)?;
        if old_status != area.status {
            let key = postcard::to_allocvec(&(&area.name, &area.updated_at))?;
            let value = postcard::to_allocvec(&area.status)?;
            let _ = tree.insert(&key, value);
        }
    }
    Ok(())
}

async fn retrieve_status_inner(tree: &sled::Tree) -> Result<(), reqwest::Error> {
    let res = reqwest::get("https://jotihunt.nl/api/2.0/areas").await?;
    let areas: Areas = res.json().await?;
    for area in areas.data {
        if let Err(err) = update_single_status(tree, area) {
            println!("error handling area: {err}")
        }
    }
    Ok(())
}

pub async fn retrieve_status_loop(db: &Db) {
    let tree = db.open_tree("status").unwrap();

    loop {
        println!("reloading status");
        if let Err(err) = retrieve_status_inner(&tree).await {
            println!("error getting status: {err}");
        }

        // every minute
        sleep(Duration::from_secs(60)).await;
    }
}
