use futures::{channel::mpsc, SinkExt, StreamExt};
use gloo::{
    net::{http::Request, websocket::futures::WebSocket},
    utils::document,
};
use jotihunt_client::update::AtomicEdit;
use state::{Address, Fox, State};
use sycamore::{
    futures::{spawn_local, spawn_local_scoped},
    prelude::*,
};

mod comms;
mod state;
mod update;

async fn location_editor() {
    let hostname = "server.lucasholten.com:4848";
    let pass_address = format!("https://{}/secret", hostname);
    let res = Request::get(&pass_address)
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await;
    let key = res.unwrap().text().await.unwrap();

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

            let ws_address = format!("wss://{}/{key}", hostname);
            let ws = WebSocket::open(&ws_address).unwrap();

            let (write, read) = ws.split();
            let (queue_write, queue_read) = mpsc::unbounded();

            spawn_local_scoped(cx, comms::write_data(queue_read, write));
            spawn_local_scoped(cx, comms::read_data(read, data));

            let queue_write = create_ref(cx, queue_write);
            let new_fox = create_signal(cx, "".to_owned());

            view! {cx,
                div(class="field") {
                    label(for="time_stamp"){"Time stamp:"}
                    input(id="time_stamp", bind:value=current_time, list="time_stamps", size=10)
                }
                datalist(id="time_stamps"){
                    Keyed(
                        iterable=slice_names,
                        view=move |cx, key| {
                            view!{cx, option(value=key)}
                        },
                        key=|key| key.clone(),
                    )
                }
                Keyed(
                    iterable=old_values,
                    view=move|cx, (key, fox)| {
                        let (key2, fox2) = (key.clone(), fox.clone());
                        let fox2 = create_ref(cx, fox2);
                        let latitude = create_signal(cx, fox.latitude);
                        let longitude = create_signal(cx, fox.longitude);
                        view!{cx,
                            div(class="field") {
                                label{(key.fox_name.clone())}
                                input(size=5, bind:value=latitude, updated={
                                    latitude.get().as_ref()==&fox2.latitude
                                })
                                input(size=5, bind:value=longitude, updated={
                                    longitude.get().as_ref()==&fox2.longitude
                                })
                                input(type="button", value="Update", on:click=move |_|{
                                    let edit = AtomicEdit{
                                        key: postcard::to_stdvec(&key2).unwrap(),
                                        old: postcard::to_stdvec(fox2).unwrap(),
                                        new: postcard::to_stdvec(&Fox{
                                            latitude: latitude.get().as_ref().trim().to_string(),
                                            longitude: longitude.get().as_ref().trim().to_string()
                                        }).unwrap(),
                                    };
                                    spawn_local_scoped(cx, async {queue_write.clone().send(edit).await.unwrap();});
                                })
                            }
                        }
                    },
                    key=|(key, fox)| (key.clone(), fox.clone())
                )
                div(class="field"){
                    input(size=10, bind:value=new_fox)
                    input(type="button", value="Add", on:click=move |_|{
                        let edit = AtomicEdit{
                            key: postcard::to_stdvec(&Address{
                                time_slice: current_time.get().as_ref().clone(),
                                fox_name: new_fox.get().as_ref().clone()
                            }).unwrap(),
                            old: vec![],
                            new: postcard::to_stdvec(&Fox::default()).unwrap()
                        };
                        spawn_local_scoped(cx, async {queue_write.clone().send(edit).await.unwrap();});
                    })
                    input(type="button", value="Del", on:click=move |_|{
                        let address = Address{
                            time_slice: current_time.get().as_ref().clone(),
                            fox_name: new_fox.get().as_ref().clone()
                        };
                        if let Some(old_fox) = data.get().get(&address) {
                            let edit = AtomicEdit{
                                key: postcard::to_stdvec(&address).unwrap(),
                                old: postcard::to_stdvec(old_fox).unwrap(),
                                new: vec![]
                            };
                            spawn_local_scoped(cx, async {queue_write.clone().send(edit).await.unwrap();});
                        }
                    })
                }
            }
        },
        &coord_editor,
    );
}

fn main() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));

    spawn_local(location_editor());
}
