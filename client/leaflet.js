
function make_map() {
    var map = L.map('map', {
        center: [52.247, 5.669],
        zoom: 7
    });

    L.tileLayer('https://tile.openstreetmap.org/{z}/{x}/{y}.png', {
        maxZoom: 19,
        attribution: '© OpenStreetMap'
    }).addTo(map);

    return map;
}
var map = make_map()

proj4.defs("EPSG:7415", "+proj=sterea +lat_0=52.1561605555556 +lon_0=5.38763888888889 +k=0.9999079 +x_0=155000 +y_0=463000 +ellps=bessel +units=m +vunits=m +no_defs +type=crs");
export function add_marker(lat, lng, name) {
    var coord = proj4("EPSG:7415", "EPSG:4326", [lat, lng]);
    return L.marker([coord[1], coord[0]]).addTo(map)
        .bindTooltip(name);
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