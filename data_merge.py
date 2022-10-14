import json
  
# Opening JSON file
geo = open('locations.geojson')
data = open('subscriptions.json')
  
data = json.load(data)
geo = json.load(geo)

def get_area(name: str) -> str:
    for thing in data["data"]:
        if thing["name"] == name:
            return thing["area"]
    assert False, name

for f in geo["features"]:
    name = f["properties"]["name"]
    del f["properties"]["description"]
    f["properties"]["area"] = get_area(name)

with open('locations_out.geojson', 'w', encoding='utf-8') as f:
    json.dump(geo, f, ensure_ascii=False)