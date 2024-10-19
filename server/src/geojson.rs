use std::{collections::HashMap, sync::Arc, time::Duration};

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
    let map: HashMap<_, _> = vec![
        ("Scouting de Paulus Dreumel", "Delta"),
        ("Scouting Phoenix Tiel", "Delta"),
        ("Scouting Graaf van Gelre Geldermalsen", "Delta"),
        ("Sint Stanislaus", "Delta"),
        ("Scouting Scherpenzeel e.o.", "Delta"),
        ("Scouting Lunteren", "Delta"),
        ("Scouting Jan Hilgers", "Delta"),
        ("Tarcisius Ede / Nijkerk", "Delta"),
        ("Scouting Langenberggroep Ede", "Delta"),
        ("Scouting Bennekom", "Delta"),
        ("Scouting Elst", "Bravo"),
        ("Schutgraaf", "Bravo"),
        ("KaLiG", "Bravo"),
        ("Scoutinggroep Lido '76", "Bravo"),
        ("Rhedense Pioniers", "Bravo"),
        ("Scouting Dieren", "Bravo"),
        ("Scoutinggroep de Markesteen", "Bravo"),
        ("Scouting Aerendheem", "Bravo"),
        ("St. Christoforus Lichtdraagsters Arnhem", "Bravo"),
        ("Scouting Zetten", "Bravo"),
        ("Scouting Valburg", "Bravo"),
        ("Karmijngroep Winssen", "Echo"),
        ("RDB", "Echo"),
        ("Scouting Beuningen '76", "Echo"),
        ("OPV Schoonoord", "Echo"),
        ("De Geuzen Arnhem", "Echo"),
        ("Andre de Thaye", "Echo"),
        ("Velpsche Woudloopers", "Echo"),
        ("Scouting St. Franciscus", "Echo"),
        ("Castor creators", "Echo"),
        ("Scouting Grave en Boxmeer", "Echo"),
        ("Scouting Woezik", "Echo"),
        ("Karel de Stoute", "Foxtrot"),
        ("Scouting Keizer Karel NIJMEGEN", "Foxtrot"),
        ("Scouting Paul Kruger", "Foxtrot"),
        ("Scouting Amalgama", "Foxtrot"),
        ("Bricks & Scouts", "Foxtrot"),
        ("Scouting Dannenburcht", "Foxtrot"),
        ("Scouting Duiven", "Foxtrot"),
        ("Scouting Groessen en Vrienden", "Foxtrot"),
        ("Subliem Hunting Team", "Foxtrot"),
        ("Scouting Bemmel", "Foxtrot"),
        ("St. Willibrordgroep Didam", "Foxtrot"),
    ]
    .into_iter()
    .map(|(a, b)| (a.to_owned(), b.to_owned()))
    .collect();

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
                "area": map.get(&group.name),
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
