use std::{sync::Arc, time::Duration};

use arc_swap::ArcSwap;
use serde::Deserialize;
use serde_json::json;
use tokio::time::sleep;

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

async fn get_geo() -> reqwest::Result<String> {
    let res = reqwest::get("https://jotihunt.nl/api/2.0/subscriptions").await?;
    let sub: Subscriptions = res.json().await?;

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

pub async fn get_reloading_geojson() -> Arc<ArcSwap<String>> {
    let geojson = get_geo().await.unwrap();
    let geojson = Arc::new(ArcSwap::new(Arc::new(geojson)));
    tokio::spawn(reload_geojson(geojson.clone()));
    geojson
}
