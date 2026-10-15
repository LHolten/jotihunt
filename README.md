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
- **Installable.** On a phone the map can be added to the home screen, where it opens full screen like an app.

## Project layout

The repository is a Cargo workspace with three crates:

| Crate | What it is |
| --- | --- |
| [server/](server/) | [axum](https://github.com/tokio-rs/axum) server with a SQLite database, which also polls the jotihunt.nl API. |
| [client/](client/) | Single page app in Rust compiled to WebAssembly, using [sycamore](https://sycamore.dev) and [Leaflet](https://leafletjs.com). |
| [shared/](shared/) | The types that go over the wire, used by both. |

The client and the server are hosted separately: the client is a static site on GitHub Pages and talks to the server on its own domain.

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

## Areas

The areas (deelgebieden) are taken from the jotihunt.nl API, so nothing needs to change when their number does. Every area gets a color by its place in the alphabetically sorted list.

Once it is known which areas we are not allowed to hunt, put their names in a file `excluded_areas` next to `password`, one per line (case does not matter):

```text
Bravo
Oscar
```

The server reads it again every minute and logs names that do not match an area. The groups in those areas are drawn grey after the client reloads the page.

[flake.nix](flake.nix) has a package for the server and a NixOS module that sets up the systemd service, a `jotihunt` user, the state directory `/var/lib/jotihunt` and a Caddy virtual host for `jotihunt.lucasholten.com`:

```nix
imports = [ jotihunt.nixosModules.${system}.default ];
services.caddy.enable = true;
```

## Traccar

To show a hunter on the map, install the Traccar client on their phone and set the server url to `https://<server>/traccar`. The exact url is also shown in the "Opties" panel of the client. No account is needed, the device identifier is used as the name on the map.

## Roadmap

Planned work and known problems are in [todo.md](todo.md).
