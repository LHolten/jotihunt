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

// `pin.svg` in `color`, the size of the leaflet marker, its tip is at (12.5, 41)
function pin_svg(color) {
    return `<svg viewBox="0 0 25 41" style="color: ${color}"><use href="pin.svg#pin"/></svg>`;
}

function make_map() {
    let map = L.map("map", {
        center: [52.1139, 5.8402],
        zoom: 10,
        zoomControl: false,
    });

    // bottom corners stack upwards, so the scale goes below the zoom buttons
    L.control
        .scale({
            position: "bottomleft",
            imperial: false,
            maxWidth: 120,
        })
        .addTo(map);

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
            groups = L.geoJSON(data, {
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
                    const marker = L.marker(latlng, { opacity: excluded ? 0.5 : 1 })
                        .setIcon(icon)
                        .bindPopup(make_popup(latlng.lat, latlng.lng));
                    // the name box is tinted with the area color, `.group-name` reads `--area`
                    marker.on("tooltipopen", (e) =>
                        e.tooltip
                            .getElement()
                            ?.style.setProperty("--area", area_color(area)),
                    );
                    return marker;
                },
            }).addTo(map);
            show_group_names();

            const unknown = data.features.some(
                (feature) => !area_names.includes(feature.properties.area),
            );
            add_legend(unknown);
        });
}

// the participant markers, set by `load_participants`
let groups;
// set by `set_group_names`, from `group_names_zoom` on the group names are always shown,
// below it (or when disabled) only on hover
let group_names_enabled = true;
let group_names_zoom = 14;
let group_names_shown;

export function set_group_names(enabled, zoom) {
    group_names_enabled = enabled;
    group_names_zoom = zoom;
    show_group_names();
    show_live_names();
}

function show_group_names() {
    if (!groups) {
        return;
    }
    const shown = group_names_enabled && map.getZoom() >= group_names_zoom;
    if (shown === group_names_shown) {
        return;
    }
    group_names_shown = shown;
    // `permanent` can only be set when binding
    const options = shown
        ? { permanent: true, direction: "bottom", className: "group-name" }
        : {};
    groups.eachLayer((marker) => {
        marker
            .unbindTooltip()
            .bindTooltip(text_tooltip(marker.feature.properties.name), options);
    });
}
map.on("zoomend", () => {
    show_group_names();
    show_live_names();
});

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

// the current status of each area and since when, set by `set_area_status`
const area_status = new Map();
// the status element in the legend row of each area, filled by `show_area_status`
const status_elements = new Map();
const statuses = {
    green: { name: "groen", color: "#2aad27" },
    orange: { name: "oranje", color: "#f39c12" },
    red: { name: "rood", color: "#e02020" },
};

export function set_area_status(area, status, since) {
    area_status.set(area, { status, since: Date.parse(since) });
    show_area_status();
}

// "32m", "1u05m" or "2d"
function format_duration(ms) {
    const mins = Math.max(0, Math.floor(ms / 60000));
    if (mins < 60) {
        return `${mins}m`;
    }
    if (mins < 24 * 60) {
        return `${Math.floor(mins / 60)}u${String(mins % 60).padStart(2, "0")}m`;
    }
    return `${Math.floor(mins / (24 * 60))}d`;
}

// a dot in the color of the status, and how long it has had it unless it is green
function show_area_status() {
    const now = Date.now();
    for (const [area, { status, since }] of area_status) {
        const element = status_elements.get(area);
        if (!element) {
            continue;
        }
        const known = statuses[status];
        element.hidden = false;
        element.style.setProperty("--status", known?.color ?? no_color);
        element.title = known?.name ?? status;
        element.textContent = status === "green" ? "" : format_duration(now - since);
    }
}
// the durations count up
setInterval(show_area_status, 30 * 1000);

// shows the color and current status of each area, `unknown` adds a row for groups without a known area
function add_legend(unknown) {
    const legend = L.control({ position: "bottomright" });
    legend.onAdd = () => {
        const div = L.DomUtil.create("div", "legend");
        const add_row = (color, label, excluded) => {
            const row = L.DomUtil.create("div", excluded ? "excluded" : "", div);
            L.DomUtil.create("span", "pin", row).innerHTML = pin_svg(color);
            row.append(label);
            return row;
        };
        for (const area of area_names) {
            // the area keeps its color for the foxes, only its groups are grey
            const excluded = excluded_areas.has(area);
            const row = add_row(area_color(area), area, excluded);
            // hidden until the status is known
            const status = L.DomUtil.create("span", "status", row);
            status.hidden = true;
            status_elements.set(area, status);
        }
        show_area_status();
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

// how long the trail behind a live location takes to fade out
const trail_ms = 60 * 1000;
// gps jitters by several meters, only moves this long count as movement
const move_m = 20;
// the live locations, set by `add_live_marker`, to show their names when zoomed in
const live_markers = new Set();

// a color from the name, so everyone sees the same color for the same location.
// Blue is left out, it is our own location
function live_color(name) {
    // fnv-1a hash
    let hash = 2166136261;
    for (const c of name) {
        hash = Math.imul(hash ^ c.codePointAt(0), 16777619);
    }
    let hue = (hash >>> 0) % 300;
    if (hue >= 190) {
        hue += 60;
    }
    return `hsl(${hue}, 85%, 45%)`;
}

// an arrow pointing in the direction of travel (degrees clockwise from north),
// or a dot while the direction is not known yet. Styled by `.live-arrow`
function live_icon(bearing, color) {
    const shape =
        bearing === null
            ? `<circle cx="12" cy="12" r="6"/>`
            : `<path d="M12 2L19.5 20L12 15.5L4.5 20Z"/>`;
    const rotate = bearing === null ? "" : `transform: rotate(${bearing}deg);`;
    return L.divIcon({
        className: "live-arrow",
        html: `<svg viewBox="0 0 24 24" style="color: ${color}; ${rotate}">${shape}</svg>`,
        iconSize: [24, 24],
        iconAnchor: [12, 12],
        tooltipAnchor: [12, 0],
        popupAnchor: [0, -12],
    });
}

// the direction from `from` to `to` in degrees clockwise from north, flat is fine for short distances
function bearing(from, to) {
    const dx = (to.lng - from.lng) * Math.cos((from.lat * Math.PI) / 180);
    const dy = to.lat - from.lat;
    return (Math.atan2(dx, dy) * 180) / Math.PI;
}

// a traccar location, with an arrow in the direction it moves and a trail that fades out
export function add_live_marker(lng, lat, name) {
    const latlng = L.latLng(lat, lng);
    const color = live_color(name);
    const live = {
        name,
        color,
        marker: L.marker(latlng, { icon: live_icon(null, color) })
            .bindTooltip(text_tooltip(name))
            .bindPopup(make_popup(lat, lng))
            .addTo(map),
        // where the trail continues from, only moves on when the location really changed
        anchor: { latlng, time: Date.now() },
        // the segments of the trail, oldest first
        trail: [],
        named: false,
    };
    live.timer = setInterval(() => fade_trail(live), 1000);
    live_markers.add(live);
    show_live_names();
    return live;
}

export function move_live_marker(live, lng, lat) {
    const latlng = L.latLng(lat, lng);
    const now = Date.now();
    const anchor = live.anchor;
    // a phone that stands still keeps sending its location, and gps jitters
    if (map.distance(anchor.latlng, latlng) >= move_m) {
        live.marker.setIcon(live_icon(bearing(anchor.latlng, latlng), live.color));
        // no line through the gap when the phone was silent for a while
        if (now - anchor.time < trail_ms) {
            const line = L.polyline([anchor.latlng, latlng], {
                color: live.color,
                weight: 4,
                interactive: false,
            }).addTo(map);
            live.trail.push({ line, time: now });
        }
        live.anchor = { latlng, time: now };
    }
    live.marker.setLatLng(latlng).setPopupContent(make_popup(lat, lng));
    fade_trail(live);
}

function fade_trail(live) {
    const now = Date.now();
    while (live.trail.length > 0 && now - live.trail[0].time >= trail_ms) {
        map.removeLayer(live.trail.shift().line);
    }
    for (const { line, time } of live.trail) {
        line.setStyle({ opacity: 0.8 * (1 - (now - time) / trail_ms) });
    }
}

export function remove_live_marker(live) {
    clearInterval(live.timer);
    live_markers.delete(live);
    map.removeLayer(live.marker);
    for (const { line } of live.trail) {
        map.removeLayer(line);
    }
}

// the names of the live locations, shown like the group names from the zoom level on
function show_live_names() {
    const shown = group_names_enabled && map.getZoom() >= group_names_zoom;
    for (const live of live_markers) {
        if (shown === live.named) {
            continue;
        }
        live.named = shown;
        const options = shown
            ? { permanent: true, direction: "bottom", className: "group-name" }
            : {};
        live.marker.unbindTooltip().bindTooltip(text_tooltip(live.name), options);
    }
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

// my location: a blue dot where the phone is. While the map follows it, the map is centered on
// the dot. Panning the map stops following, the dot stays, the button starts following again.
const my_color = "#4285f4";
const follow_zoom = 16;
let my_dot = null;
let my_accuracy = null;
let following = false;
let watch_started = false;
let locate_link;

function show_my_location(latlng, accuracy) {
    if (!my_dot) {
        my_accuracy = L.circle(latlng, {
            radius: accuracy,
            color: my_color,
            weight: 1,
            fillColor: my_color,
            fillOpacity: 0.15,
            interactive: false,
        }).addTo(map);
        my_dot = L.circleMarker(latlng, {
            radius: 8,
            color: "white",
            weight: 3,
            fillColor: my_color,
            fillOpacity: 1,
            interactive: false,
        }).addTo(map);
        if (following) {
            map.setView(latlng, Math.max(map.getZoom(), follow_zoom));
        }
    }
    my_dot.setLatLng(latlng);
    my_accuracy.setLatLng(latlng).setRadius(accuracy);
    if (following) {
        map.panTo(latlng);
    }
}

function start_watching_location() {
    watch_started = true;
    navigator.geolocation.watchPosition(
        (pos) => {
            const { latitude, longitude, accuracy } = pos.coords;
            show_my_location(L.latLng(latitude, longitude), accuracy);
        },
        () => {
            if (following) {
                following = false;
                update_locate_link();
                alert("could not get your location");
            }
        },
        { enableHighAccuracy: true },
    );
}

function toggle_following() {
    following = !following;
    if (following) {
        if (!watch_started) {
            start_watching_location();
        }
        if (my_dot) {
            map.setView(my_dot.getLatLng(), Math.max(map.getZoom(), follow_zoom));
        }
    }
    update_locate_link();
}

function update_locate_link() {
    locate_link.classList.toggle("following", following);
}

// stop following when the user pans, the dot stays where it is
map.on("dragstart", () => {
    if (following) {
        following = false;
        update_locate_link();
    }
});

// the button under the zoom buttons, the icon is the crosshair of most map apps
const locate_svg = `<svg viewBox="0 0 24 24"><path d="M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8zm8.94 3A8.994 8.994 0 0 0 13 3.06V1h-2v2.06A8.994 8.994 0 0 0 3.06 11H1v2h2.06A8.994 8.994 0 0 0 11 20.94V23h2v-2.06A8.994 8.994 0 0 0 20.94 13H23v-2h-2.06zM12 19c-3.87 0-7-3.13-7-7s3.13-7 7-7 7 3.13 7 7-3.13 7-7 7z"/></svg>`;

function add_locate_control() {
    const control = L.control({ position: "bottomleft" });
    control.onAdd = () => {
        const bar = L.DomUtil.create("div", "leaflet-bar");
        L.DomEvent.disableClickPropagation(bar);
        locate_link = L.DomUtil.create("a", "locate", bar);
        locate_link.title = "Mijn locatie";
        locate_link.setAttribute("role", "button");
        locate_link.innerHTML = locate_svg;
        L.DomEvent.on(locate_link, "click", (e) => {
            L.DomEvent.preventDefault(e);
            toggle_following();
        });
        return bar;
    };
    control.addTo(map);
}
add_locate_control();
