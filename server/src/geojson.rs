use std::{sync::Arc, time::Duration};

use serde::Deserialize;
use serde_json::json;
use tokio::time::sleep;

use crate::AppState;

#[derive(Deserialize)]
struct Subscriptions {
    data: Vec<Group>,
}

#[derive(Deserialize)]
struct Group {
    name: String,
    lat: String,
    long: String,
    area: Option<String>,
}

pub async fn get_geo(state: &'static AppState) -> anyhow::Result<String> {
    let res = state.fetch("subscriptions").await?;
    let sub: Subscriptions = serde_json::from_str(&res)?;

    let mut features = vec![];
    for group in sub.data {
        if group.name.to_lowercase().contains("test") {
            continue;
        }

        features.push(json!({
            "type": "Feature",
            "geometry": {
                "type": "Point",
                "coordinates": [group.long, group.lat]
            },
            "properties": {
                "name": group.name,
                "area": group.area,
            },
        }))
    }
    let geo = json!({
        "type": "FeatureCollection",
        "features": features
    });

    Ok(serde_json::to_string(&geo).unwrap())
}

pub async fn reload_geojson_loop(state: &'static AppState) {
    loop {
        println!("reloading geojson");
        match get_geo(state).await {
            Ok(new) => state.geojson.store(Arc::new(new)),
            Err(err) => println!("error getting geojson: {err:#}"),
        }
        // every hour
        sleep(Duration::from_secs(60 * 60)).await;
    }
}
