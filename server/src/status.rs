use std::{fs::read_to_string, sync::Arc, time::Duration};

use jotihunt_shared::domain::{Area, StatusKey};
use serde::Deserialize;
use tokio::time::sleep;

use crate::{db::Status, encode_broadcast, AppState};

#[derive(Deserialize)]
struct Areas {
    data: Vec<ApiArea>,
}

#[derive(Deserialize)]
struct ApiArea {
    name: String,
    status: String,
    updated_at: String,
}

async fn update_single_status(state: &'static AppState, area: ApiArea) {
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

/// One area name per line, case does not matter. It is read again on every poll,
/// so it can be changed during the hunt without a restart.
const EXCLUDED_AREAS_FILE: &str = "excluded_areas";

/// The areas we are not allowed to hunt, in lowercase. A missing file means none.
fn read_excluded_areas() -> Vec<String> {
    read_to_string(EXCLUDED_AREAS_FILE)
        .unwrap_or_default()
        .lines()
        .map(|line| line.trim().to_lowercase())
        .filter(|line| !line.is_empty())
        .collect()
}

/// Sorted by name, so every client gives each area the same color.
fn area_list(mut names: Vec<String>, excluded: &[String]) -> Vec<Area> {
    names.sort();
    names.dedup();
    for name in excluded {
        if !names.iter().any(|n| n.to_lowercase() == *name) {
            println!("{EXCLUDED_AREAS_FILE}: there is no area called {name:?}");
        }
    }
    names
        .into_iter()
        .map(|name| Area {
            huntable: !excluded.contains(&name.to_lowercase()),
            name,
        })
        .collect()
}

/// Update the status table and the lists of areas.
pub async fn retrieve_status(state: &'static AppState) -> Result<(), reqwest::Error> {
    let res = reqwest::get("https://jotihunt.nl/api/2.0/areas").await?;
    let areas: Areas = res.json().await?;
    let mut names = vec![];
    for area in areas.data {
        names.push(area.name.clone());
        update_single_status(state, area).await;
    }
    let areas = area_list(names, &read_excluded_areas());
    let names: Vec<_> = areas.iter().map(|area| &area.name).collect();
    state
        .fox_list
        .store(Arc::new(serde_json::to_string(&names).unwrap()));
    state
        .areas
        .store(Arc::new(serde_json::to_string(&areas).unwrap()));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excluded_areas() {
        let names = ["Oscar", "Bravo", "Alpha"].map(str::to_owned).to_vec();
        let areas = area_list(names, &["bravo".to_owned(), "zulu".to_owned()]);
        let area = |name: &str, huntable| Area {
            name: name.to_owned(),
            huntable,
        };
        assert_eq!(
            areas,
            [
                area("Alpha", true),
                area("Bravo", false),
                area("Oscar", true)
            ]
        );
    }
}
