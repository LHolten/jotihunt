use std::{collections::BTreeMap, future::pending, rc::Rc, time::Duration};

use comms::{live_updated, LocationEdit};
use gloo::{
    dialogs::alert,
    net::http::{Request, RequestBuilder, Response},
    storage::{LocalStorage, Storage},
    timers::future::sleep,
    utils::{document, window},
};
use jotihunt_shared::{
    domain::{Fox, FoxKey},
    ModifyLocation,
};
use js_sys::Date;
use leaflet::Line;
use options::option_panel;
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

fn location_editor(key: &'static str, fox_names: Vec<String>) {
    let coord_editor = document()
        .get_element_by_id("coord_editor")
        .expect("there is a add_point button");

    sycamore::render_to(
        move || {
            let (data, send_edit) = live_updated::<FoxKey, Fox>(key, "locations");

            let current_time = create_signal(String::new());

            let current_day = {
                let date = Date::new_0();
                let day = date.get_date();
                // javascript months start at 0
                let month = date.get_month() + 1;
                let year = date.get_full_year();
                create_signal(format!("{year:0>4}-{month:0>2}-{day:0>2}"))
            };

            let check_day = move |k: &FoxKey| current_day.with(|day| *day == k.day);

            let old_values = create_memo(move || {
                let time = current_time.get_clone();
                data.with(|data| {
                    data.iter()
                        .filter(|(k, _)| k.time == time && check_day(k))
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect::<Vec<_>>()
                })
            });

            {
                let today_by_fox = create_memo(move || {
                    let mut today_by_fox: BTreeMap<_, Vec<_>> = BTreeMap::new();
                    data.with(|data| {
                        data.iter().for_each(|(k, v)| {
                            if check_day(k) {
                                today_by_fox
                                    .entry(k.fox_name.clone())
                                    .or_default()
                                    .push((k.time.clone(), v.clone()));
                            }
                        })
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
                            marker.set_fox(true);
                            line.push(&marker);
                            markers.push(marker);
                        }
                    }
                    if let Some(last) = markers.last() {
                        last.set_fox(false);
                    }
                    Rc::new((fox_name, line, markers))
                });
            }

            let new_fox = create_signal(fox_names[0].clone());

            let slice_names = create_memo(move || {
                let mut names: Vec<_> = data.with(|data| {
                    data.keys()
                        .filter(|k| check_day(k))
                        .map(|k| k.time.clone())
                        .collect()
                });
                names.push(current_time.get_clone());
                names.sort();
                names.dedup();
                names
            });

            let area = create_signal(fox_names[0].clone());

            let fox_options = || {
                let options = fox_names.iter().map(|name| {
                    let label = name.clone();
                    let name = name.clone();
                    view! { option(value=name) {(label)} }
                });
                View::from(options.collect::<Vec<_>>())
            };
            let fox_options_clone = fox_options();
            let fox_options = fox_options();

            let hunt_coord = create_signal(String::new());

            view! {
                summary {"Coordinaten"}
                div(class="field") {
                    input(bind:value=hunt_coord, placeholder="xxxx, yyyy of 51.xxx, 4.yyy of 52°xx'xx.x\"N 4°yy'yy.y\"E")
                }
                div(class="field") {
                    select(bind:value=area) {(fox_options)}
                    input(r#type="time", bind:value=current_time)
                    input(r#type="button", value="Toevoegen", on:click=move |_| {
                        let hunt_coord = hunt_coord.get_clone();
                        if current_time.with(String::is_empty) {
                            alert("geen tijd geselecteerd");
                            return
                        }
                        if area.with(String::is_empty) {
                            alert("geen vos geselecteerd");
                            return
                        }
                        let Some((lat, long)) = hunt_coord.split_once(',')
                            .or_else(|| hunt_coord.trim().split_once(char::is_whitespace)) else {
                            alert("coordinaat heeft geen comma of spatie");
                            return
                        };
                        send_edit(LocationEdit::Create(
                            FoxKey {
                                day: current_day.get_clone(),
                                time: current_time.get_clone(),
                                fox_name: area.get_clone(),
                            },
                            Fox{
                                latitude: lat.trim().to_string(),
                                longitude: long.trim().to_string()
                            },
                        ));
                    })
                }

                details {
                    summary {"Bewerken"}
                    div(class="field") {
                        select(bind:value=current_time) {
                            Keyed(
                                list=slice_names,
                                view=move |key| {
                                    let selected = current_time.with(|time| *time == key);
                                    let label = key.clone();
                                    view!{ option(value=key, selected=selected){(label)} }
                                },
                                key=|key| key.clone(),
                            )
                        }
                        input(r#type="date", bind:value=current_day)
                    }
                    hr()
                    Keyed(
                        list=old_values,
                        view=move |(key, fox)| {
                            let fox_name = key.fox_name.clone();
                            let latitude = create_signal(fox.latitude.clone());
                            let longitude = create_signal(fox.longitude.clone());
                            // the signal is only used as a `Copy` handle to the old value
                            let old = create_signal((key, fox));
                            let new = move || Fox {
                                latitude: latitude.with(|l| l.trim().to_string()),
                                longitude: longitude.with(|l| l.trim().to_string()),
                            };
                            let send_update = move || {
                                let (key, old) = old.get_clone();
                                send_edit(LocationEdit::Modify(key, ModifyLocation { old, new: new() }));
                            };
                            view!{
                                div(class="field") {
                                    input(r#type="button", value=fox_name, on:click=move |_|{
                                        if let Some(marker) = comms::make_marker(&new(), "zoom") {
                                            marker.set_color("grey");
                                            marker.zoom_to();
                                            spawn_local_scoped(async {
                                                sleep(Duration::from_secs(1)).await;
                                                drop(marker)
                                            });
                                        }
                                    })
                                    input(size="7", bind:value=latitude, placeholder="xxxx", "updated"=move || {
                                        old.with(|(_, old)| latitude.with(|l| *l == old.latitude)).to_string()
                                    }, on:change=move |_|{
                                        send_update();
                                    })
                                    input(size="7", bind:value=longitude, placeholder="yyyy", "updated"=move || {
                                        old.with(|(_, old)| longitude.with(|l| *l == old.longitude)).to_string()
                                    }, on:change=move |_|{
                                        send_update();
                                    })
                                }
                            }
                        },
                        key=|(key, fox)| (key.clone(), fox.clone())
                    )
                    hr()
                    div(class="field"){
                        select(bind:value=new_fox) {(fox_options_clone)}
                        input(r#type="button", value="Verwijderen", on:click=move |_|{
                            if current_time.with(String::is_empty) {
                                alert("Selecteer eerst een tijdstip");
                                return;
                            }
                            let new_fox = new_fox.get_clone();
                            let old_values = old_values.get_clone();
                            let Some((_, old_value)) = old_values.iter().find(|x| x.0.fox_name == new_fox) else {
                                alert("Er is geen coordinaat in dat deelgebied");
                                return
                            };

                            if !old_value.latitude.is_empty() || !old_value.longitude.is_empty() {
                                alert("Alleen lege coordinaten kunnen verwijderd worden");
                                return;
                            }

                            let address = FoxKey{
                                day: current_day.get_clone(),
                                time: current_time.get_clone(),
                                fox_name: new_fox,
                            };
                            send_edit(LocationEdit::Delete(address, Fox::default()));
                        })
                    }
                }
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

    leaflet::load_participants(&format!("https://{HOSTNAME}/deelnemers.geojson"));

    spawn_local(async {
        let key = Box::leak(retrieve_secret().await.into_boxed_str());

        let fox_names: Vec<String> =
            gloo::net::http::Request::get(&format!("https://{HOSTNAME}/fox_list.json"))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();

        location_editor(key, fox_names);
        option_panel(key);
        articles::articles();
    });
}
