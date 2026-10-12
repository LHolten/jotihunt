# Jotihunt

A shared live map for a scouting group taking part in the [Jotihunt](https://jotihunt.nl), the yearly Dutch fox hunt. Everyone at home base and in the cars sees the same map: where each fox was spotted, where the hunters are, and the hints and status changes published by the organisation.

The interface is in Dutch.

## What it does

- **Fox locations.** Enter the coordinates of a fox for a given time. Every fox gets a trail of markers on the map, connected by a line in the colour of its area. Edits show up for everyone right away.
- **Three coordinate formats.** Rijksdriehoek as used in the hints (`xxxx, yyyy`, four digits each), decimal degrees (`51.954, 5.872`) or degrees, minutes and seconds (`52°00'35.4"N 4°25'08.7"E`).
- **Timeline.** Hints, and optionally all other messages and the status changes of the areas, are taken from the jotihunt.nl API. A new item is announced with a popup and spoken text.
- **Participating groups.** All groups are drawn on the map with a 500 m circle, coloured by area.
- **Live hunters.** Phones running a [Traccar client](https://www.traccar.org/client/) report their position and are shown as a person on the map. A marker disappears after five minutes without an update.
- **Own location.** The browser can show where you are.

## How it is built

The repository is a Cargo workspace with three crates:

| Crate | What it is |
| --- | --- |
| [server/](server/) | [axum](https://github.com/tokio-rs/axum) server. Stores everything in SQLite through [rust-query](https://github.com/LHolten/rust-query) and polls the jotihunt.nl API. |
| [client/](client/) | Single page app in Rust compiled to WebAssembly, using [sycamore](https://sycamore.dev) and built with [Trunk](https://trunkrs.dev). The map is [Leaflet](https://leafletjs.com), wrapped in [client/leaflet.js](client/leaflet.js). |
| [shared/](shared/) | The types that go over the wire, used by both. |

The client and the server are hosted separately: the client is a static site on GitHub Pages and talks to the server on its own domain.

### Live data

The server keeps three tables (locations, statuses and articles). Each table has a [server sent events](https://developer.mozilla.org/en-US/docs/Web/API/Server-sent_events) endpoint that first sends `Reset`, then every row, and after that every change as it happens. The client keeps a sorted map per table up to date from that stream, and the browser reconnects by itself.

Changes are broadcast inside the database transaction that makes them, and a new connection subscribes inside the transaction that reads its snapshot, so a client never misses a change or sees them out of order.

Location edits are safe to do concurrently: an update or delete includes the value the client thinks is current, and the server refuses it with `409` if someone else changed it in the meantime.

### Login

There is a single password for the whole group. The login form posts it to the server, which redirects back to the client with a secret in the url fragment. The client stores the secret in `localStorage` and uses it as the first path segment of every protected endpoint. The secret is new every time the server starts, so everyone has to log in again after a restart.

### Server endpoints

| Endpoint | | |
| --- | --- | --- |
| `POST /login` | public | Form with `password` and `redirect`, redirects back with the secret |
| `GET /status` | public | Event stream of the area statuses |
| `GET /articles` | public | Event stream of the articles (title and type only) |
| `GET /articles/{publish_at}/{id}` | public | HTML body of one article |
| `GET /deelnemers.geojson` | public | The participating groups |
| `GET /fox_list.json` | public | The names of the areas |
| `ANY /traccar` | public | Position report from a Traccar client |
| `GET /{secret}/check` | secret | Checks if a stored secret is still valid |
| `GET /{secret}/locations` | secret | Event stream of the fox locations |
| `POST`, `PUT`, `DELETE /{secret}/locations/{day}/{time}/{fox_name}` | secret | Create, modify or delete a fox location |
| `GET /{secret}/live` | secret | Event stream of the Traccar positions |

`/traccar` accepts either the query parameters `id`, `lat` and `lon`, or a JSON body with `device_id` and `location.coords.latitude` / `longitude`.

The server fetches the area statuses every minute, the articles every five seconds and the participating groups every hour.

## Running it locally

You need:

- Rust with the wasm target: `rustup target add wasm32-unknown-unknown`
- [Trunk](https://trunkrs.dev): `cargo install trunk`
- [Caddy](https://caddyserver.com), which the debug server starts itself to get https on localhost. `nix develop` provides it.
- On Windows a C compiler (the MSVC build tools), because SQLite is compiled from source there. On Linux and macOS the system SQLite is used.

Start the server:

```sh
cd server
cargo run
```

It listens on `https://localhost:3000`. On the first run it creates two files in the working directory: `joti.sqlite` with the database and `password` with the password `test`. Change the contents of `password` to set your own. The server needs to reach jotihunt.nl when it starts.

Start the client in a second terminal:

```sh
cd client
trunk serve
```

Open the address that Trunk prints. A debug build of the client talks to `localhost:3000`, a release build to the production server; this is the `HOSTNAME` constant in [client/src/main.rs](client/src/main.rs).

Run the tests with `cargo test`.

## Deployment

**Client.** Every push to `master` builds the client with `trunk build --release` and publishes it to GitHub Pages, see [.github/workflows/main.yml](.github/workflows/main.yml). To host it for another server, change `HOSTNAME`.

**Server.** A release build on Linux or macOS listens on the unix socket `/run/jotihunt/socket` and expects a reverse proxy in front of it that takes care of https. It reads `password` and `joti.sqlite` from its working directory.

[flake.nix](flake.nix) has a package for the server and a NixOS module that sets up the systemd service, a `jotihunt` user, the state directory `/var/lib/jotihunt` and a Caddy virtual host for `jotihunt.lucasholten.com`:

```nix
imports = [ jotihunt.nixosModules.${system}.default ];
services.caddy.enable = true;
```

## Traccar

To show a hunter on the map, install the Traccar client on their phone and set the server url to `https://<server>/traccar`. The exact url is also shown in the "Opties" panel of the client. No account is needed, the device identifier is used as the name on the map.

## Roadmap

Planned work and known problems are in [todo.md](todo.md).
