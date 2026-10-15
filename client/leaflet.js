function marker_url(color) {
    return `https://raw.githubusercontent.com/pointhi/leaflet-color-markers/master/img/marker-icon-2x-${color}.png`;
}

function make_icon(color) {
    return new L.Icon({
        iconUrl: marker_url(color),
        shadowUrl:
            "https://cdnjs.cloudflare.com/ajax/libs/leaflet/0.7.7/images/marker-shadow.png",
        iconSize: [25, 41],
        iconAnchor: [12, 41],
        popupAnchor: [1, -34],
        shadowSize: [41, 41],
    });
}

// the colors of the areas, in the order of `/areas.json`
const palette = [
    "#cb2b3e", // red
    "#2aad27", // green
    "#2a81cb", // blue
    "#cac428", // yellow
    "#3d3d3d", // black
    "#1fa5a0", // teal
    "#cb8427", // orange
    "#9c2bcb", // violet
    "#e05fb4", // pink
    "#8a5a2b", // brown
    "#1c3f94", // navy
    "#7fb800", // lime
];
// groups without a known area, and groups in an area we are not allowed to hunt
const no_color = "#7b7b7b";

// set by `set_areas`, before anything is drawn
let area_names = [];
let excluded_areas = new Set();

export function set_areas(names, excluded) {
    area_names = names;
    excluded_areas = new Set(excluded);
}

function area_color(area) {
    const i = area_names.indexOf(area);
    if (i < 0) {
        return no_color;
    }
    if (i < palette.length) {
        return palette[i];
    }
    // more areas than colors, spread the rest over the color wheel
    return `hsl(${(i * 137.5) % 360}, 60%, 45%)`;
}

// a map pin in `color`, the size of the leaflet marker, its tip is at (12.5, 41)
function pin_svg(color) {
    return `<svg viewBox="0 0 25 41"><path fill="${color}" stroke="rgba(0, 0, 0, 0.4)" d="M12.5 0.5C5.9 0.5 0.5 5.9 0.5 12.5C0.5 21.5 12.5 40.5 12.5 40.5S24.5 21.5 24.5 12.5C24.5 5.9 19.1 0.5 12.5 0.5Z"/><circle cx="12.5" cy="12.5" r="4.5" fill="white"/></svg>`;
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

    let options = {
        maxZoom: 19,
        attribution: "© OpenStreetMap",
    };
    let osm = L.tileLayer(
        "https://tile.openstreetmap.org/{z}/{x}/{y}.png",
        options,
    );

    if (L.Browser.retina) {
        // osm has no high resolution tiles, these are the same map at 512px.
        // The server is only tolerated for outside use and can stop without
        // notice, so the first tile that fails switches back to osm.
        // That is blurry, osm tiles can only be sharp by drawing them at half size.
        let hd = L.tileLayer("https://{s}.osm.rrze.fau.de/osmhd/{z}/{x}/{y}.png", {
            ...options,
            subdomains: "ab",
        });
        hd.once("tileerror", () => {
            map.removeLayer(hd);
            osm.addTo(map);
        });
        hd.addTo(map);
    } else {
        osm.addTo(map);
    }

    return map;
}
var map = make_map();

export function load_participants(url) {
    fetch(url)
        .then((res) => res.json())
        .then((data) => {
            L.geoJSON(data, {
                pointToLayer: function (feature, latlng) {
                    const area = feature.properties.area;
                    const excluded = excluded_areas.has(area);
                    const circle = { radius: 500, opacity: 0.1 };
                    if (excluded) {
                        circle.color = no_color;
                    }
                    L.circle(latlng, circle).addTo(map);
                    const f = 0.5;
                    const icon = L.divIcon({
                        className: "pin",
                        html: pin_svg(excluded ? no_color : area_color(area)),
                        iconSize: [25 * f, 41 * f],
                        iconAnchor: [12.5 * f, 41 * f],
                        popupAnchor: [0, -34 * f],
                    });
                    return L.marker(latlng, { opacity: excluded ? 0.5 : 1 })
                        .setIcon(icon)
                        .bindTooltip(text_tooltip(feature.properties.name))
                        .bindPopup(make_popup(latlng.lat, latlng.lng));
                },
            }).addTo(map);

            const unknown = data.features.some(
                (feature) => !area_names.includes(feature.properties.area),
            );
            add_legend(unknown);
        });
}

// leaflet renders a string tooltip as html, but names like traccar identifiers are user input
function text_tooltip(name) {
    const span = L.DomUtil.create("span");
    span.textContent = name;
    return span;
}

// the coordinates, with links that open the route to them in google maps
// without an origin google maps starts at the current location
function make_popup(lat, lng) {
    const div = L.DomUtil.create("div");
    div.append([lat, lng].toString());
    const links = L.DomUtil.create("div", "", div);
    const modes = { driving: "Route met de auto", walking: "Route lopend" };
    for (const [mode, label] of Object.entries(modes)) {
        const link = L.DomUtil.create("a", "", links);
        link.href = `https://www.google.com/maps/dir/?api=1&destination=${lat},${lng}&travelmode=${mode}`;
        link.target = "_blank";
        link.rel = "noopener";
        link.textContent = label;
        links.append(" ");
    }
    return div;
}

// shows the color of each area, `unknown` adds a row for groups without a known area
function add_legend(unknown) {
    const legend = L.control({ position: "bottomright" });
    legend.onAdd = () => {
        const div = L.DomUtil.create("div", "legend");
        const add_row = (color, label, excluded) => {
            const row = L.DomUtil.create("div", excluded ? "excluded" : "", div);
            L.DomUtil.create("span", "pin", row).innerHTML = pin_svg(color);
            row.append(label);
        };
        for (const area of area_names) {
            // the area keeps its color for the foxes, only its groups are grey
            const excluded = excluded_areas.has(area);
            add_row(area_color(area), excluded ? `${area} (niet jagen)` : area, excluded);
        }
        if (unknown) {
            add_row(no_color, "onbekend", false);
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
        .bindTooltip(text_tooltip(name))
        .bindPopup(make_popup(coord[1], coord[0]))
        .addTo(map);
    return marker;
}

export function remove_layer(marker) {
    map.removeLayer(marker);
}

export function new_line(fox) {
    return L.polyline([], { color: area_color(fox) }).addTo(map);
}

export function add_line_marker(line, marker) {
    line.addLatLng(marker.getLatLng());
}

export function set_marker_color(marker, color) {
    let icon = make_icon(color);
    marker.setIcon(icon);
}

export function set_human(marker) {
    set_custom(marker, "human.png");
}

// `fox.svg` in the color of the area, its tip is at (16, 35), styled by `.fox-pin`
export function set_fox(marker, fox, old) {
    const f = 0.75;
    const color = area_color(fox);
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
