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

function fox_color(fox) {
    let color_map = {
        "a": "violet",
        "b": "gold",
        "c": "red",
        "d": "blue",
        "e": "green",
        "f": "black",
    }

    if (fox == null) {
        return "grey"
    }
    let char = fox.charAt(0).toLowerCase()
    return color_map[char] ?? "gray"
}

function make_map() {
    let map = L.map('map', {
        center: [52.1139, 5.8402],
        zoom: 10,
        zoomControl: false,
    });

    L.control.zoom({
        position: 'bottomleft',
    }).addTo(map);

    L.tileLayer('https://tile.openstreetmap.org/{z}/{x}/{y}.png', {
        maxZoom: 19,
        attribution: '© OpenStreetMap'
    }).addTo(map);

    fetch("https://jotihunt.lucasholten.com/deelnemers.geojson").then(res => res.json()).then(data => {
        L.geoJSON(data, {
            pointToLayer: function (feature, latlng) {
                return L.marker(latlng)
                    .setIcon(make_icon(false, fox_color(feature.properties.area)))
                    .bindTooltip(feature.properties.name);
            }
        }).addTo(map);
    });

    return map;
}
var map = make_map()

let orga = add_marker(5.8725166117126575, 51.95402844147237, "orga", false);
set_custom(orga, "/stikkerbuilding.png");

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

export function new_line(fox) {
    return L.polyline([], { color: fox_color(fox) }).addTo(map);
}

export function add_line_marker(line, marker) {
    line.addLatLng(marker.getLatLng())
}

export function set_marker_color(marker, color) {
    let icon = make_icon(true, color);
    marker.setIcon(icon);
}

export function set_human(marker) {
    set_custom(marker, '/human.png')
}

function set_custom(marker, name) {
    let f = 0.75;
    let icon = new L.Icon({
        iconUrl: name,
        iconSize: [34 * f, 41 * f],
        iconAnchor: [17 * f, 41 * f],
        popupAnchor: [1 * f, -34 * f],
        shadowSize: [41 * f, 41 * f]
    });
    marker.setIcon(icon);
}

export function zoom_to(marker) {
    map.flyTo(marker.getLatLng())
}