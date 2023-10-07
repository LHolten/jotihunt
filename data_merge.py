import json

# Opening JSON file
with open('subscriptions.json') as file:
    data = json.load(file)

geo = {
    "type": "FeatureCollection",
    "features": []
}

for f in data["data"]:
    geo["features"].append({
        "type": "Feature", 
        "geometry": {
            "type": "Point", 
            "coordinates": [f["long"], f["lat"]]
        }, 
        "properties": {
            "name": f["name"], 
            "area": f["area"],
        },
    })


with open('locations_out.geojson', 'w', encoding='utf-8') as file:
    json.dump(geo, file, ensure_ascii=False)