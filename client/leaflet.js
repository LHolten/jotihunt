
export function make_map() {
    var map = L.map('map', {
        center: [51.505, -0.09],
        zoom: 13
    });

    L.tileLayer('https://tile.openstreetmap.org/{z}/{x}/{y}.png', {
        maxZoom: 19,
        attribution: '© OpenStreetMap'
    }).addTo(map);

    return map;
}

export function add_marker(map, lat, lng) {
    return L.marker([lat, lng]).addTo(map);
}