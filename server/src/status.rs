use std::{collections::BTreeSet, fs::read_to_string, sync::Arc, time::Duration};

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

/// One area name per line. It is read again on every poll,
/// so it can be changed during the hunt without a restart.
const EXCLUDED_AREAS_FILE: &str = "excluded_areas";

/// The areas we are not allowed to hunt. A missing file means none.
fn read_excluded_areas() -> BTreeSet<String> {
    let file = read_to_string(EXCLUDED_AREAS_FILE).unwrap_or_else(|err| {
        println!("{EXCLUDED_AREAS_FILE}: {err}, all areas are huntable");
        String::new()
    });
    file.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Sorted by name, so every client gives each area the same color.
fn area_list(names: BTreeSet<String>, excluded: &BTreeSet<String>) -> Vec<Area> {
    for name in excluded.difference(&names) {
        println!("{EXCLUDED_AREAS_FILE}: there is no area called {name:?}");
    }
    names
        .into_iter()
        .map(|name| Area {
            huntable: !excluded.contains(&name),
            name,
        })
        .collect()
}

/// Update the status table and the list of areas.
pub async fn retrieve_status(state: &'static AppState) -> anyhow::Result<()> {
    let res = state.fetch("areas").await?;
    let areas: Areas = serde_json::from_str(&res)?;
    let mut names = BTreeSet::new();
    for area in areas.data {
        names.insert(area.name.clone());
        update_single_status(state, area).await;
    }
    let areas = area_list(names, &read_excluded_areas());
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
            println!("error getting status: {err:#}");
            sleep(Duration::from_secs(60)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excluded_areas() {
        let set = |names: &[&str]| names.iter().copied().map(str::to_owned).collect();
        let areas = area_list(set(&["Oscar", "Bravo", "Alpha"]), &set(&["Bravo", "Zulu"]));
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
