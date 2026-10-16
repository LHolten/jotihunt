# Jotihunt

A shared live map for a scouting group taking part in the [Jotihunt](https://jotihunt.nl), the yearly Dutch fox hunt. Everyone at home base and in the cars sees the same map: where each fox was spotted, where the hunters are, and the hints and status changes published by the organisation.

The interface is in Dutch.

## About the Jotihunt

The Jotihunt is a 26-hour game for scouting groups in Gelderland, run by volunteers on the third weekend of October, alongside JOTA-JOTI. It starts on Saturday at 10:00 and ends on Sunday at 12:00. The game is aimed at Scouts aged 14 and up (Explorers and older). The rules and news are published on [jotihunt.nl](https://jotihunt.nl), see the [spelregels](https://jotihunt.nl/page/spelregels), the [FAQ](https://jotihunt.nl/page/faq) and the [begrippenlijst](https://jotihunt.nl/page/begrippenlijst).

- **Foxes and areas.** Teams of three foxes (a *vossenteam*) walk through Gelderland. Each team belongs to an area (*deelgebied*, also called a cluster) named after it, such as Alpha, and visits every group in that area. At 23:00 on Saturday the foxes are swapped for fresh ones, which splits the game into two halves of 13 hours. Before the game each group hears which foxes it may hunt.
- **Hints.** Every hour the organisation publishes a hint per fox: a puzzle whose answer is usually the fox's position as an eight-digit Rijksdriehoek coordinate (`1234 5678`), sometimes a word. Solving it quickly also earns a point.
- **Hunts.** Hunters find a fox in person and get a code from them, which must be entered on the website within 15 minutes. A hunt is worth 3 points, or 6 on the fox of your own area, and double during the announced *Happy Hour*. The same fox may be hunted once an hour, and foxes may never be followed.
- **Fox status.** Each fox team has a status on the website: green (active) and orange (on the move) may be hunted, red (inactive, for example during their one long move per half) may not.
- **Counter-hunts.** The foxes also try to place a *tegenhunt* sticker within 500 m of each group's home base. The group then gets a compass direction via Telegram and has 30 minutes to find the sticker.
- **Assignments.** Throughout the weekend the organisation publishes creative assignments (photo, video or text) that earn points too.

The group with the most points on Sunday wins the *wisseltrofee*. The public [API](https://jotihunt.nl/page/api) lists the participating groups, the status of each area and all published articles (hints, assignments and news). It is rate limited to 30 calls per minute; going over it gets you blocked.

## What it does

- **Fox locations.** Pick a time, by default the last whole hour of a hint or "nu" for a hunt, and enter the coordinates of each area. Every fox gets a trail of markers on the map, connected by a line in the colour of its area. Edits show up for everyone right away.
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

**Server.** A release build on Linux or macOS listens on the unix socket `/run/jotihunt/socket` and expects a reverse proxy in front of it that takes care of https. It reads `password` and `joti.sqlite` from its working directory, and `excluded_areas` if it exists: the areas we may not hunt, one per line, whose groups are drawn grey.

[flake.nix](flake.nix) has a package for the server and a NixOS module that sets up the systemd service, a `jotihunt` user, the state directory `/var/lib/jotihunt` and a Caddy virtual host for `jotihunt.lucasholten.com`:

```nix
imports = [ jotihunt.nixosModules.${system}.default ];
services.caddy.enable = true;
```

## Traccar

To show a hunter on the map, install the Traccar client on their phone and set the server url to `https://<server>/traccar`. The exact url is also shown in the "Opties" panel of the client. No account is needed, the device identifier is used as the name on the map.

## Roadmap

Planned work and known problems are in [todo.md](todo.md).
