use std::{collections::BTreeMap, future::pending, rc::Rc, time::Duration};

use comms::{live_updated, LocationEdit};
use gloo::{
    dialogs::{alert, confirm},
    net::http::{Request, RequestBuilder, Response},
    storage::{LocalStorage, Storage},
    timers::future::sleep,
    utils::{document, window},
};
use jotihunt_shared::{
    domain::{Area, Fox, FoxKey},
    ModifyLocation,
};
use js_sys::Date;
use leaflet::Line;
use options::{option_panel, traccar_panel};
use sycamore::{
    futures::{spawn_local, spawn_local_scoped},
    prelude::*,
};

use wasm_bindgen::{JsCast, JsValue};
use web_sys::{HtmlFormElement, HtmlInputElement, Url};

mod articles;
mod comms;
mod leaflet;
mod options;

cfg_select! {
    debug_assertions => {
        const HOSTNAME: &str = "localhost:3000";
    }
    _ => {
        const HOSTNAME: &str = "jotihunt.lucasholten.com";
    }
}

/// `day` and `time` in the formats of the date and time inputs, as a local date.
fn slot_date(day: &str, time: &str) -> Option<Date> {
    let mut day = day.split('-').map(str::parse::<u32>);
    let mut time = time.split(':').map(str::parse::<u32>);
    let (Some(Ok(year)), Some(Ok(month)), Some(Ok(date))) = (day.next(), day.next(), day.next())
    else {
        return None;
    };
    let (Some(Ok(hours)), Some(Ok(minutes))) = (time.next(), time.next()) else {
        return None;
    };
    // javascript months start at 0
    Some(Date::new_with_year_month_day_hr_min(
        year,
        month as i32 - 1,
        date as i32,
        hours as i32,
        minutes as i32,
    ))
}

/// The day and time of `date` in the formats of the date and time inputs.
fn slot_of(date: &Date) -> (String, String) {
    let day = format!(
        "{:0>4}-{:0>2}-{:0>2}",
        date.get_full_year(),
        // javascript months start at 0
        date.get_month() + 1,
        date.get_date()
    );
    let time = format!("{:0>2}:{:0>2}", date.get_hours(), date.get_minutes());
    (day, time)
}

/// The whole hour at or before `date`.
fn whole_hour(date: &Date) -> Date {
    let hour = Date::new(&date.get_time().into());
    hour.set_minutes(0);
    hour.set_seconds(0);
    hour.set_milliseconds(0);
    hour
}

const HOUR_MS: f64 = 60. * 60. * 1000.;

/// The coordinate as it is typed in the field, `None` when the field is empty.
/// The two sides are split by a comma, or by a space for degrees, minutes and seconds.
fn parse_field(text: &str) -> Option<Fox> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let (lat, long) = text
        .split_once(',')
        .or_else(|| text.split_once(char::is_whitespace))
        .unwrap_or((text, ""));
    Some(Fox {
        latitude: lat.trim().to_string(),
        longitude: long.trim().to_string(),
    })
}

fn show_field(fox: &Fox) -> String {
    format!("{}, {}", fox.latitude, fox.longitude)
}

/// The coordinate of one area in the slot of `day` and `time`.
/// It is saved when the field is left, emptying the field deletes it.
/// Areas we are not allowed to hunt are grey, like their groups on the map.
fn coord_row(
    area: Area,
    day: Signal<String>,
    time: Signal<String>,
    data: ReadSignal<BTreeMap<FoxKey, Fox>>,
    send_edit: impl Fn(LocationEdit) + Copy + 'static,
) -> View {
    let Area {
        name: fox_name,
        huntable,
    } = area;
    let label = fox_name.clone();
    let id = format!("coord_{fox_name}");
    let label_for = id.clone();
    // the signal is only used as a `Copy` handle to the name
    let fox_name = create_signal(fox_name);
    let key = move || FoxKey {
        day: day.get_clone(),
        time: time.get_clone(),
        fox_name: fox_name.get_clone(),
    };
    // only updates when the coordinate of this area in this slot changes
    let saved = create_selector(move || {
        let key = key();
        data.with(|data| data.get(&key).cloned())
    });

    // what is in the text field
    let text = create_signal(String::new());
    // write data from upstream in the field, also when another slot is selected
    create_effect(move || {
        day.track();
        time.track();
        text.set(saved.with(|saved| saved.as_ref().map(show_field).unwrap_or_default()));
    });
    let typed = move || text.with(|text| parse_field(text));

    let save = move |new: Option<Fox>| {
        if time.with(String::is_empty) {
            alert("Kies eerst een tijd");
            return;
        }
        let key = key();
        match (saved.get_clone(), new) {
            (old, new) if old == new => {}
            (Some(old), None) => {
                let question = format!(
                    "Coördinaat van {} om {} verwijderen?",
                    key.fox_name, key.time
                );
                if confirm(&question) {
                    send_edit(LocationEdit::Delete(key, old));
                } else {
                    text.set(show_field(&old));
                }
            }
            (None, Some(new)) => send_edit(LocationEdit::Create(key, new)),
            (Some(old), Some(new)) => {
                send_edit(LocationEdit::Modify(key, ModifyLocation { old, new }))
            }
            (None, None) => unreachable!("equal, so handled by the first arm"),
        }
    };

    let on_change = move |_| match typed() {
        // an invalid coordinate is not saved, the field keeps it with a cross
        Some(fox) if !comms::is_valid(&fox) => {}
        new => save(new),
    };

    let valid = move || typed().map(|fox| comms::is_valid(&fox));
    let status = move || match valid() {
        Some(true) => "✓",
        Some(false) => "✗",
        None => "",
    };

    let zoom = move |_| {
        if let Some(marker) = typed().and_then(|fox| comms::make_marker(&fox, "zoom")) {
            marker.set_color("grey");
            marker.zoom_to();
            spawn_local_scoped(async {
                sleep(Duration::from_secs(1)).await;
                drop(marker)
            });
        }
    };

    view! {
        div(class="coord_row", "huntable"=huntable.to_string()) {
            label(r#for=label_for) {(label)}
            input(
                id=id,
                bind:value=text,
                placeholder="xxxx, yyyy",
                title="RD (xxxx, yyyy), graden (51.954, 5.872) of graden, minuten en seconden (52°00'35.4\"N 4°25'08.7\"E)",
                "updated"=move || (typed() == saved.get_clone()).to_string(),
                on:change=on_change,
            )
            button(
                class="status",
                title="Inzoomen",
                disabled=move || valid() != Some(true),
                on:click=zoom,
            ) {(status)}
        }
    }
}

fn location_editor(key: &'static str, areas: Vec<Area>) {
    let coord_editor = document()
        .get_element_by_id("coord_editor")
        .expect("there is a add_point button");

    sycamore::render_to(
        move || {
            let (data, send_edit) = live_updated::<FoxKey, Fox>(key, "locations");

            // the slot that is edited, hints are on whole hours so it starts at the last one
            let (day, time) = slot_of(&whole_hour(&Date::new_0()));
            let current_day = create_signal(day);
            let current_time = create_signal(time);

            {
                let today_by_fox = create_memo(move || {
                    let day = current_day.get_clone();
                    let mut today_by_fox: BTreeMap<_, Vec<_>> = BTreeMap::new();
                    data.with(|data| {
                        for (k, v) in data.iter().filter(|(k, _)| k.day == day) {
                            today_by_fox
                                .entry(k.fox_name.clone())
                                .or_default()
                                .push((k.time.clone(), v.clone()));
                        }
                    });
                    today_by_fox.into_iter().collect::<Vec<_>>()
                });

                // the lines and markers are removed from the map when they are dropped
                map_indexed(today_by_fox, |(fox_name, points)| {
                    let line = Line::new(&fox_name);
                    let mut markers = vec![];
                    for (time, fox) in points {
                        let name = format!("{} ({})", fox_name, time);
                        if let Some(marker) = comms::make_marker(&fox, &name) {
                            marker.set_fox(&fox_name, true);
                            line.push(&marker);
                            markers.push(marker);
                        }
                    }
                    if let Some(last) = markers.last() {
                        last.set_fox(&fox_name, false);
                    }
                    Rc::new((fox_name, line, markers))
                });
            }

            // goes to the previous or next whole hour, or to a slot that already has
            // coordinates when that is closer, like the time of a hunt
            let step = move |forward: bool| {
                let current = (current_day.get_clone(), current_time.get_clone());
                let date = slot_date(&current.0, &current.1).unwrap_or_else(Date::new_0);
                let hour = whole_hour(&date).get_time();
                let hour = if forward {
                    hour + HOUR_MS
                } else if hour < date.get_time() {
                    hour
                } else {
                    hour - HOUR_MS
                };
                let hour = slot_of(&Date::new(&hour.into()));
                let saved = data.with(|data| {
                    let slots = data.keys().map(|k| (k.day.clone(), k.time.clone()));
                    if forward {
                        slots.filter(|slot| *slot > current).min()
                    } else {
                        slots.filter(|slot| *slot < current).max()
                    }
                });
                let (day, time) = match saved {
                    Some(saved) if forward => saved.min(hour),
                    Some(saved) => saved.max(hour),
                    None => hour,
                };
                current_day.set(day);
                current_time.set(time);
            };

            // a hunt is at an exact time
            let now = move |_| {
                let (day, time) = slot_of(&Date::new_0());
                current_day.set(day);
                current_time.set(time);
            };

            let day_label = move || {
                let day = current_day.get_clone();
                let Some(date) = slot_date(&day, "00:00") else {
                    return day;
                };
                let weekday = ["zo", "ma", "di", "wo", "do", "vr", "za"][date.get_day() as usize];
                // javascript months start at 0
                format!("{weekday} {}-{}", date.get_date(), date.get_month() + 1)
            };

            let rows = areas
                .into_iter()
                .map(|area| coord_row(area, current_day, current_time, data, send_edit))
                .collect::<Vec<_>>();

            view! {
                summary {"Coordinaten"}
                div(class="slot") {
                    button(title="Vorig uur", on:click=move |_| step(false)) {"◀"}
                    input(r#type="time", bind:value=current_time)
                    button(title="Volgend uur", on:click=move |_| step(true)) {"▶"}
                    button(title="De tijd van nu, voor een hunt", on:click=now) {"nu"}
                    span(class="day") {(day_label)}
                }
                (rows)
            }
        },
        &coord_editor,
    );
}

const SECRET_KEY: &str = "secret";

/// Returns the secret that is needed for all location endpoints.
///
/// The secret is stored, so the password is only needed again after the server
/// restarts with a new secret. Without a valid secret the login form is shown and
/// this never returns, submitting the form leaves the page.
async fn retrieve_secret() -> String {
    // the login redirects back with the result in the fragment, it is removed
    // so it does not end up in bookmarks or the history
    let url = Url::new(&window().location().href().unwrap()).unwrap();
    let fragment = url.hash();
    if !fragment.is_empty() {
        url.set_hash("");
        let history = window().history().unwrap();
        history
            .replace_state_with_url(&JsValue::NULL, "", Some(&url.href()))
            .unwrap();
    }

    if let Some(secret) = fragment.strip_prefix("#secret=") {
        LocalStorage::set(SECRET_KEY, secret).unwrap();
    }

    if fragment == "#wrong" {
        alert("verkeerd wachtwoord");
    }

    if let Ok(secret) = LocalStorage::get::<String>(SECRET_KEY) {
        let check = format!("https://{HOSTNAME}/{secret}/check");
        if authorized(|| Request::get(&check)).await.is_some() {
            return secret;
        }
    }

    show_login(&url.href());
    pending().await
}

/// Sends the request until the server answers, `None` means it was unauthorized.
async fn authorized(request: impl Fn() -> RequestBuilder) -> Option<Response> {
    loop {
        match request().send().await {
            Ok(res) if res.ok() => return Some(res),
            Ok(res) if res.status() == 401 => return None,
            Ok(res) => alert(&format!("inloggen mislukt: {}", res.status_text())),
            Err(err) => alert(&format!("inloggen mislukt: {err}")),
        }
        sleep(Duration::from_secs(5)).await;
    }
}

/// Shows the login form, which posts the password to the server.
fn show_login(redirect: &str) {
    let element = |id| {
        document()
            .get_element_by_id(id)
            .expect("the login form exists")
    };
    let form: HtmlFormElement = element("login").unchecked_into();
    form.set_action(&format!("https://{HOSTNAME}/login"));
    element("redirect")
        .unchecked_into::<HtmlInputElement>()
        .set_value(redirect);
    form.remove_attribute("hidden").unwrap();
    let _ = element("password")
        .unchecked_into::<HtmlInputElement>()
        .focus();
}

fn main() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));

    spawn_local(async {
        let areas: Vec<Area> = Request::get(&format!("https://{HOSTNAME}/areas.json"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let name = |area: &Area| area.name.clone();
        leaflet::set_areas(
            areas.iter().map(name).collect(),
            areas
                .iter()
                .filter(|area| !area.huntable)
                .map(name)
                .collect(),
        );
        leaflet::load_participants(&format!("https://{HOSTNAME}/deelnemers.geojson"));

        let key = Box::leak(retrieve_secret().await.into_boxed_str());

        location_editor(key, areas);
        traccar_panel(key);
        option_panel();
        articles::articles();
    });
}
