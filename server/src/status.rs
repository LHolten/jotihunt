use std::{sync::Arc, time::Duration};

use jotihunt_shared::domain::StatusKey;
use serde::Deserialize;
use tokio::time::sleep;

use crate::{db::Status, encode_broadcast, AppState};

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

async fn update_single_status(state: &'static AppState, area: Area) {
    state
        .db
        .transaction_mut_ok(move |txn| {
            txn.scope(|txn| {
                let new = Status {
                    date_time: area.updated_at.clone(),
                    fox_name: area.name.clone(),
                    status: area.status.clone(),
                };
                if let Err(row) = txn.insert(new) {
                    let mut status = txn.mutable(row);
                    if status.status == area.status {
                        return;
                    }
                    status.status = area.status.clone();
                }
                let key = StatusKey {
                    date_time: area.updated_at,
                    fox_name: area.name,
                };
                let _ = state
                    .live_status
                    .send(encode_broadcast(&key, Some(&area.status)));
            })
        })
        .await
}

/// Update the status table and the list of fox names.
pub async fn retrieve_status(state: &'static AppState) -> Result<(), reqwest::Error> {
    let res = reqwest::get("https://jotihunt.nl/api/2.0/areas").await?;
    let areas: Areas = res.json().await?;
    let mut foxes = vec![];
    for area in areas.data {
        foxes.push(area.name.clone());
        update_single_status(state, area).await;
    }
    state
        .fox_list
        .store(Arc::new(serde_json::to_string(&foxes).unwrap()));
    Ok(())
}

pub async fn retrieve_status_loop(state: &'static AppState) {
    loop {
        // every minute
        sleep(Duration::from_secs(60)).await;

        println!("reloading status");
        if let Err(err) = retrieve_status(state).await {
            println!("error getting status: {err}");
        }
    }
}
