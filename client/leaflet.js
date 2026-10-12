function marker_url(color) {
    return `https://raw.githubusercontent.com/pointhi/leaflet-color-markers/master/img/marker-icon-2x-${color}.png`;
}

function make_icon(large, color) {
    let f;
    if (large) {
        f = 1;
    } else {
        f = 0.5;
    }
    return new L.Icon({
        iconUrl: marker_url(color),
        shadowUrl:
            "https://cdnjs.cloudflare.com/ajax/libs/leaflet/0.7.7/images/marker-shadow.png",
        iconSize: [25 * f, 41 * f],
        iconAnchor: [12 * f, 41 * f],
        popupAnchor: [1 * f, -34 * f],
        shadowSize: [41 * f, 41 * f],
    });
}

function fox_color(fox) {
    let color_map = {
        a: "red",
        b: "green",
        c: "blue",
        d: "yellow",
        e: "black",
        f: "grey",
        g: "orange",
        h: "violet",
    };

    if (fox == null) {
        return "grey";
    }
    let char = fox.charAt(0).toLowerCase();
    return color_map[char] ?? "grey";
}

function make_map() {
    let map = L.map("map", {
        center: [52.1139, 5.8402],
        zoom: 10,
        zoomControl: false,
    });

    L.control
        .zoom({
            position: "bottomleft",
        })
        .addTo(map);

    L.tileLayer("https://tile.openstreetmap.org/{z}/{x}/{y}.png", {
        maxZoom: 19,
        // osm has no high resolution tiles, so on high dpi screens this
        // shows the tiles of one zoom level deeper at half their size
        detectRetina: true,
        attribution: "© OpenStreetMap",
    }).addTo(map);

    return map;
}
var map = make_map();

export function load_participants(url) {
    fetch(url)
        .then((res) => res.json())
        .then((data) => {
            L.geoJSON(data, {
                pointToLayer: function (feature, latlng) {
                    L.circle(latlng, { radius: 500, opacity: 0.1 }).addTo(map);
                    const color = fox_color(feature.properties.area);
                    const icon = make_icon(false, color);
                    return L.marker(latlng)
                        .setIcon(icon)
                        .bindTooltip(feature.properties.name)
                        .bindPopup([latlng.lat, latlng.lng].toString());
                },
            }).addTo(map);

            const areas = data.features.map((feature) => feature.properties.area);
            add_legend([...new Set(areas)].sort());
        });
}

// shows the participant marker of each area
function add_legend(areas) {
    const legend = L.control({ position: "bottomright" });
    legend.onAdd = () => {
        const div = L.DomUtil.create("div", "legend");
        for (const area of areas) {
            const row = L.DomUtil.create("div", "", div);
            const icon = L.DomUtil.create("img", "", row);
            icon.src = marker_url(fox_color(area));
            row.append(area ?? "onbekend");
        }
        return div;
    };
    legend.addTo(map);
}

let orga = add_marker(5.8725166117126575, 51.95402844147237, "orga", false);
set_custom(orga, "stikkerbuilding.png", 0.5);

proj4.defs(
    "EPSG:7415",
    "+proj=sterea +lat_0=52.1561605555556 +lon_0=5.38763888888889 +k=0.9999079 +x_0=155000 +y_0=463000 +ellps=bessel +units=m +vunits=m +no_defs +type=crs",
);
export function add_marker(lat, lng, name, convert) {
    let coord = [lat, lng];
    if (convert) {
        coord = proj4("EPSG:7415", "EPSG:4326", coord);
    }
    let marker = L.marker([coord[1], coord[0]])
        .bindTooltip(name)
        .bindPopup([coord[1], coord[0]].toString())
        .addTo(map);
    return marker;
}

export function remove_layer(marker) {
    map.removeLayer(marker);
}

export function new_line(fox) {
    return L.polyline([], { color: fox_color(fox) }).addTo(map);
}

export function add_line_marker(line, marker) {
    line.addLatLng(marker.getLatLng());
}

export function set_marker_color(marker, color) {
    let icon = make_icon(true, color);
    marker.setIcon(icon);
}

export function set_human(marker) {
    set_custom(marker, "human.png");
}

// the colors of the participant markers
const marker_hex = {
    red: "#cb2b3e",
    green: "#2aad27",
    blue: "#2a81cb",
    yellow: "#cac428",
    black: "#3d3d3d",
    grey: "#7b7b7b",
    orange: "#cb8427",
    violet: "#9c2bcb",
};

// `fox.svg` in the color of the area, its tip is at (16, 35), styled by `.fox-pin`
export function set_fox(marker, fox, old) {
    const f = 0.75;
    const color = marker_hex[fox_color(fox)];
    const icon = L.divIcon({
        className: "fox-pin",
        html: `<svg viewBox="0 0 32 37" style="color: ${color}"><use href="fox.svg#fox"/></svg>`,
        iconSize: [32 * f, 37 * f],
        iconAnchor: [16 * f, 35 * f],
        popupAnchor: [0, -28 * f],
    });
    marker.setIcon(icon);
    if (old) {
        marker.setOpacity(0.5);
    } else {
        marker.setOpacity(1.0);
    }
}

function set_custom(marker, name, f = 0.75) {
    let icon = new L.Icon({
        iconUrl: name,
        iconSize: [34 * f, 41 * f],
        iconAnchor: [17 * f, 41 * f],
        popupAnchor: [1 * f, -34 * f],
        shadowSize: [41 * f, 41 * f],
    });
    marker.setIcon(icon);
}

export function zoom_to(marker) {
    map.flyTo(marker.getLatLng());
}
