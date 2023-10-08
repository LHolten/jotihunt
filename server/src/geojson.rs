use serde::Deserialize;
use serde_json::json;

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

pub async fn get_geo() -> reqwest::Result<String> {
    let res = reqwest::get("https://jotihunt.nl/api/2.0/subscriptions").await?;
    let sub: Subscriptions = res.json().await?;

    let mut features = vec![];
    for group in sub.data {
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
