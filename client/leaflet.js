function make_icon(large, color) {
    let f
    if (large) {
        f = 1
    } else {
        f = 0.5
    }
    return new L.Icon({
        iconUrl: `https://raw.githubusercontent.com/pointhi/leaflet-color-markers/master/img/marker-icon-2x-${color}.png`,
        shadowUrl: 'https://cdnjs.cloudflare.com/ajax/libs/leaflet/0.7.7/images/marker-shadow.png',
        iconSize: [25 * f, 41 * f],
        iconAnchor: [12 * f, 41 * f],
        popupAnchor: [1 * f, -34 * f],
        shadowSize: [41 * f, 41 * f]
    });
}

let color_map = {
    "Alpha": "violet",
    "Bravo": "gold",
    "Charlie": "red",
    "Delta": "blue",
    "Echo": "green",
    "Foxtrot": "black",
    null: "grey"
}

function make_map() {
    let map = L.map('map', {
        center: [52.1139, 5.8402],
        zoom: 10,
    });

    L.tileLayer('https://tile.openstreetmap.org/{z}/{x}/{y}.png', {
        maxZoom: 19,
        attribution: '© OpenStreetMap'
    }).addTo(map);

    fetch("https://gist.githubusercontent.com/LHolten/60f91a9cceed5afd4483cd1cbbf2e98d/raw/df13c88a5072dc794165dff1ea9748ff9cf69abb/jotihunt%2520data").then(res => res.json()).then(data => {
        L.geoJSON(data, {
            pointToLayer: function (feature, latlng) {
                return L.marker(latlng)
                    .setIcon(make_icon(false, color_map[feature.properties.area]))
                    .bindTooltip(feature.properties.name);
            }
        }).addTo(map);
    });

    return map;
}
var map = make_map()

proj4.defs("EPSG:7415", "+proj=sterea +lat_0=52.1561605555556 +lon_0=5.38763888888889 +k=0.9999079 +x_0=155000 +y_0=463000 +ellps=bessel +units=m +vunits=m +no_defs +type=crs");
export function add_marker(lat, lng, name, convert) {
    let coord = [lat, lng];
    if (convert) {
        coord = proj4("EPSG:7415", "EPSG:4326", coord)
    }
    let marker = L.marker([coord[1], coord[0]])
        .bindTooltip(name)
        .bindPopup([coord[1], coord[0]].toString())
        .addTo(map);
    return marker;
}

export function remove_layer(marker) {
    map.removeLayer(marker)
}

export function new_line() {
    return L.polyline([], { color: 'red' }).addTo(map);
}

export function add_line_marker(line, marker) {
    line.addLatLng(marker.getLatLng())
}

export function set_marker_color(marker, color) {
    let icon = make_icon(true, color);
    marker.setIcon(icon);
}

export function zoom_to(marker) {
    map.flyTo(marker.getLatLng())
}