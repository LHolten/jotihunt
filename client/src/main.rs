use std::{collections::BTreeMap, rc::Rc, time::Duration};

use comms::live_updated;
use futures::SinkExt;
use gloo::{dialogs::alert, net::http::Request, timers::future::sleep, utils::document};
use jotihunt_shared::{
    domain::{Fox, FoxKey},
    AtomicEdit,
};
use js_sys::Date;
use leaflet::Line;
use options::option_panel;
use sycamore::{
    futures::{spawn_local, spawn_local_scoped},
    prelude::*,
};

mod articles;
mod comms;
mod leaflet;
mod options;

const HOSTNAME: &str = "jotihunt.lucasholten.com";
const WS_PROTOCOL: &str = "wss";
const HTTP_PROTOCOL: &str = "https";

fn location_editor(key: &'static str, fox_names: &[String]) {
    let coord_editor = document()
        .get_element_by_id("coord_editor")
        .expect("there is a add_point button");

    sycamore::render_to(
        |cx| {
            let (data, queue_write) = live_updated::<FoxKey, Fox>(cx, key, "locations");

            let current_time = create_signal(cx, String::new());

            let current_day = {
                let date = Date::new_0();
                let day = date.get_date();
                let month = date.get_month();
                let year = date.get_full_year();
                create_signal(cx, format!("{year:0>4}-{month:0>2}-{day:0>2}"))
            };

            let check_day = create_ref(cx, |k: &FoxKey| &*current_day.get() == &k.day);

            let old_values = create_memo(cx, || {
                data.get()
                    .iter()
                    .filter_map(|(k, v)| {
                        let time_eq = &k.time == &*current_time.get();
                        (time_eq && check_day(k)).then_some((k.clone(), v.clone()))
                    })
                    .collect::<Vec<_>>()
            });

            {
                let today_by_fox = create_memo(cx, || {
                    let mut today_by_fox: BTreeMap<_, Vec<_>> = BTreeMap::new();
                    data.get().iter().for_each(|(k, v)| {
                        if check_day(k) {
                            today_by_fox
                                .entry(k.fox_name.clone())
                                .or_default()
                                .push((k.time.clone(), v.clone()));
                        }
                    });
                    today_by_fox.into_iter().collect()
                });

                let lines = map_indexed(cx, today_by_fox, |_cx, (fox_name, points)| {
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
                create_memo(cx, || lines.get());
            }

            let new_fox = create_signal(cx, fox_names[0].clone());

            let slice_names = create_memo(cx, || {
                let mut names: Vec<_> = data
                    .get()
                    .keys()
                    .filter_map(|k| check_day(k).then_some(k.time.clone()))
                    .collect();
                names.push(current_time.get().as_ref().to_owned());
                names.sort();
                names.dedup();
                names
            });

            let area = create_signal(cx, fox_names[0].clone());

            let fox_options = move || {
                View::new_fragment(
                    fox_names
                        .iter()
                        .map(|name| {
                            let name = create_ref(cx, name.clone());
                            view! {cx, option(value=*name){(*name)}}
                        })
                        .collect::<Vec<_>>(),
                )
            };
            let fox_options_clone = fox_options();
            let fox_options = fox_options();

            let hunt_coord = create_signal(cx, String::new());

            view! {cx,
                summary {"Coordinaten"}
                div(class="field") {
                    input(bind:value=hunt_coord, placeholder="xxxx, yyyy of 51.xxx, 4.yyy")
                }
                div(class="field") {
                    select(bind:value=area) {(fox_options)}
                    input(type="time", bind:value=current_time)
                    input(type="button", value="Toevoegen", on:click=move|_| {
                        let hunt_coord = hunt_coord.get();
                        if current_time.get().is_empty() {
                            alert("geen tijd geselecteerd");
                            return
                        }
                        if area.get().is_empty() {
                            alert("geen vos geselecteerd");
                            return
                        }
                        let Some((lat, long)) = hunt_coord.split_once(',') else {
                            alert("coordinaat heeft geen comma");
                            return
                        };
                        let edit = AtomicEdit{
                            key: postcard::to_stdvec(&FoxKey {
                                day: current_day.get().as_ref().clone(),
                                time: current_time.get().as_ref().clone(),
                                fox_name: area.get().as_ref().clone(),
                            }).unwrap(),
                            old: Vec::new(),
                            new: postcard::to_stdvec(&Fox{
                                latitude: lat.trim().to_string(),
                                longitude: long.trim().to_string()
                            }).unwrap(),
                        };
                        spawn_local_scoped(cx, async {queue_write.clone().send(edit).await.unwrap();});
                    })
                }

                details {
                    summary {"Bewerken"}
                    div(class="field") {
                        select(bind:value=current_time) {
                            Keyed(
                                iterable=slice_names,
                                view=move |cx, key| {
                                    let key = create_ref(cx, key);
                                    let selected = key == current_time.get().as_ref();
                                    view!{cx, option(value=*key, selected=selected){(*key)}}
                                },
                                key=|key| key.clone(),
                            )
                        }
                        input(type="date", bind:value=current_day)
                    }
                    hr()
                    Keyed(
                        iterable=old_values,
                        view=move|cx, (key, fox)| {
                            let (key2, fox2) = (key.clone(), fox.clone());
                            let fox2 = create_ref(cx, fox2);
                            let latitude = create_signal(cx, fox.latitude);
                            let longitude = create_signal(cx, fox.longitude);
                            let send_update = create_ref(cx, move || {
                                let edit = AtomicEdit{
                                    key: postcard::to_stdvec(&key2).unwrap(),
                                    old: postcard::to_stdvec(fox2).unwrap(),
                                    new: postcard::to_stdvec(&Fox{
                                        latitude: latitude.get().as_ref().trim().to_string(),
                                        longitude: longitude.get().as_ref().trim().to_string()
                                    }).unwrap(),
                                };
                                spawn_local_scoped(cx, async {queue_write.clone().send(edit).await.unwrap();});
                            });
                            view!{cx,
                                div(class="field") {
                                    input(type="button", value=(key.fox_name.clone()), on:click=move |_|{
                                        if let Some(marker) = comms::make_marker(
                                            &Fox {
                                                latitude: latitude.get().as_ref().trim().to_string(),
                                                longitude: longitude.get().as_ref().trim().to_string(),
                                            },
                                            "zoom",
                                        ) {
                                            marker.set_color("grey");
                                            marker.zoom_to();
                                            spawn_local_scoped(cx, async {
                                                sleep(Duration::from_secs(1)).await;
                                                drop(marker)
                                            });
                                        }
                                    })
                                    input(size=7, bind:value=latitude, placeholder="xxxx", updated={
                                        latitude.get().as_ref()==&fox2.latitude
                                    }, on:change=move |_|{
                                        send_update();
                                    })
                                    input(size=7, bind:value=longitude, placeholder="yyyy", updated={
                                        longitude.get().as_ref()==&fox2.longitude
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
                        input(type="button", value="Verwijderen", on:click=move |_|{
                            if current_time.get().is_empty() {
                                alert("Selecteer eerst een tijdstip");
                                return;
                            }
                            let new_fox = new_fox.get();
                            let old_values = old_values.get();
                            let Some((_, old_value)) = old_values.iter().find(|x|&*x.0.fox_name == &*new_fox) else {
                                alert("Er is geen coordinaat in dat deelgebied");
                                return
                            };

                            if !old_value.latitude.is_empty() || !old_value.longitude.is_empty() {
                                alert("Alleen lege coordinaten kunnen verwijderd worden");
                                return;
                            }

                            let address = FoxKey{
                                day: current_day.get().as_ref().clone(),
                                time: current_time.get().as_ref().clone(),
                                fox_name: new_fox.to_string()
                            };
                            let edit = AtomicEdit{
                                key: postcard::to_stdvec(&address).unwrap(),
                                old: postcard::to_stdvec(&Fox::default()).unwrap(),
                                new: vec![]
                            };
                            spawn_local_scoped(cx, async {queue_write.clone().send(edit).await.unwrap();});
                        })
                    }
                }
            }
        },
        &coord_editor,
    );
}

fn main() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));

    spawn_local(async {
        let pass_address = format!("{HTTP_PROTOCOL}://{HOSTNAME}/secret");
        let res = Request::get(&pass_address)
            .credentials(web_sys::RequestCredentials::Include)
            .send()
            .await;
        let key = res.unwrap().text().await.unwrap();
        let key = Box::leak(key.into_boxed_str());

        let fox_names: Vec<String> =
            gloo::net::http::Request::get(&format!("{HTTP_PROTOCOL}://{HOSTNAME}/fox_list.json"))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();

        location_editor(key, &fox_names);
        option_panel(key);
        articles::articles(key);
    });
}

#[test]
fn update_keyed() {
    create_scope_immediate(|cx| {
        let a = create_signal(cx, vec![("a", 1), ("b", 2), ("c", 3)]);
        let mapped = map_keyed(cx, a, |_, x| x.1 * 2, |x| x.0);
        assert_eq!(*mapped.get(), vec![2, 4, 6]);

        a.set(vec![("a", 0), ("b", 0), ("c", 0)]);
        assert_eq!(*mapped.get(), vec![0, 0, 0]);
    });
}
