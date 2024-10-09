use std::time::Duration;

use futures::{channel::mpsc, SinkExt, StreamExt};
use gloo::{
    net::{http::Request, websocket::futures::WebSocket},
    timers::future::sleep,
    utils::document,
};
use jotihunt_shared::AtomicEdit;
use options::option_panel;
use state::{Address, Fox, State};
use sycamore::{
    futures::{spawn_local, spawn_local_scoped},
    prelude::*,
};

mod comms;
mod leaflet;
mod options;
mod state;

const HOSTNAME: &str = "server.lucasholten.com:4848";
const WS_PROTOCOL: &str = "wss";
const HTTP_PROTOCOL: &str = "https";

fn location_editor(key: &'static str) {
    let coord_editor = document()
        .get_element_by_id("coord_editor")
        .expect("there is a add_point button");

    let state = State::default();

    sycamore::render_to(
        |cx| {
            let data = create_signal(cx, state.data);
            let current_time = create_signal(cx, state.current_time);
            let old_values = create_memo(cx, || {
                data.get()
                    .iter()
                    .filter_map(|(k, v)| {
                        k.time_slice
                            .eq(&*current_time.get())
                            .then_some((k.clone(), v.clone()))
                    })
                    .collect::<Vec<_>>()
            });
            let slice_names = create_memo(cx, || {
                let mut names: Vec<_> = data.get().keys().map(|k| k.time_slice.clone()).collect();
                names.dedup();
                names
            });

            let ws_address = format!("{WS_PROTOCOL}://{HOSTNAME}/{key}");
            let ws = WebSocket::open(&ws_address).unwrap();

            let (write, read) = ws.split();
            let (queue_write, queue_read) = mpsc::unbounded();

            spawn_local_scoped(cx, comms::write_data(queue_read, write));
            spawn_local_scoped(cx, comms::read_data(read, data));

            let queue_write = create_ref(cx, queue_write);
            let new_fox = create_signal(cx, "".to_owned());
            let new_timestamp = create_signal(cx, "".to_owned());

            let time_stamps = move || {
                view! {cx,
                    option(value=current_time.get(), selected=true, hidden=true){(current_time.get())}
                    Keyed(
                        iterable=slice_names,
                        view=move |cx, key| {
                            let key = create_ref(cx, key);
                            view!{cx, option(value=*key){(*key)}}
                        },
                        key=|key| key.clone(),
                    )
                }
            };

            view! {cx,
                summary {"Coordinaten"}
                div(class="field") {
                    label(for="time_stamp"){"Tijdstip van de hint:  "}
                    select(id="time_stamp", bind:value=current_time) {(time_stamps())}
                }
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
                details {
                    summary {"Tijdstippen en Vossen bewerken"}
                    div(class="field") {
                        input(size=8, bind:value=new_timestamp, placeholder="hh:mm")
                        input(type="button", value="Selecteer tijdstip", on:click=move |_|{
                            current_time.set(new_timestamp.get().as_str().to_owned());
                        })
                    }
                    div(class="field"){
                        input(size=15, bind:value=new_fox, placeholder="alpha, bravo, charlie")
                        input(type="button", value="Toevoegen", on:click=move |_|{
                            for name in new_fox.get().split(',') {
                                if name.trim().is_empty() {
                                    continue;
                                }
                                let edit = AtomicEdit{
                                    key: postcard::to_stdvec(&Address{
                                        time_slice: current_time.get().as_ref().clone(),
                                        fox_name: name.trim().to_string()
                                    }).unwrap(),
                                    old: vec![],
                                    new: postcard::to_stdvec(&Fox::default()).unwrap()
                                };
                                spawn_local_scoped(cx, async {queue_write.clone().send(edit).await.unwrap();});
                            }
                        })
                        input(type="button", value="Verwijderen", on:click=move |_|{
                            for name in new_fox.get().split(',') {
                                let address = Address{
                                    time_slice: current_time.get().as_ref().clone(),
                                    fox_name: name.trim().to_string()
                                };
                                if let Some(old_fox) = data.get().get(&address) {
                                    let edit = AtomicEdit{
                                        key: postcard::to_stdvec(&address).unwrap(),
                                        old: postcard::to_stdvec(old_fox).unwrap(),
                                        new: vec![]
                                    };
                                    spawn_local_scoped(cx, async {queue_write.clone().send(edit).await.unwrap();});
                                }
                            }
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

        location_editor(key);
        option_panel(key);
    });
}
