use futures::{channel::mpsc, future, SinkExt, StreamExt, TryStreamExt};
use gloo::{
    dialogs::prompt,
    events::EventListener,
    net::websocket::{futures::WebSocket, Message},
    utils::document,
};
use jotihunt_client::update::{AtomicEdit, Broadcast};
use state::{Address, Fox, State};
// use mk_geolocation::callback::Position;
use sycamore::{futures::spawn_local_scoped, prelude::*};
use wasm_bindgen::prelude::*;

mod state;
mod update;

#[wasm_bindgen(module = "/leaflet.js")]
extern "C" {
    type Map;

    fn make_map() -> Map;

    type Marker;

    fn add_marker(map: &Map, lat: f64, lng: f64) -> Marker;

}

fn location_editor() {
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

            let ws = WebSocket::open("ws://localhost:8090").unwrap();
            // let password = prompt("password", None).unwrap();
            let (write, read) = ws.split();

            let (queue_write, queue_read) = mpsc::unbounded();
            let queue_write = create_ref(cx, queue_write);

            spawn_local_scoped(cx, async {
                queue_read
                    .map(|edit| {
                        let msg = Message::Bytes(postcard::to_stdvec(&edit).unwrap());
                        Ok(msg)
                    })
                    .forward(write)
                    .await
                    .unwrap();
            });

            spawn_local_scoped(cx, async move {
                read.try_for_each(|msg| match msg {
                    Message::Text(_) => panic!("we want bytes"),
                    Message::Bytes(bin) => {
                        let broadcast: Broadcast = postcard::from_bytes(&bin).unwrap();
                        data.modify().insert(
                            postcard::from_bytes(&broadcast.key).unwrap(),
                            postcard::from_bytes(&broadcast.value).unwrap(),
                        );
                        future::ok(())
                    }
                })
                .await
                .unwrap();
            });

            view! {cx,
                input(bind:value=current_time, list="time_stamps")
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
                        let latitude = create_signal(cx, fox.latitude);
                        let longitude = create_signal(cx, fox.longitude);
                        view!{cx,
                            div(class="field") {
                                label{(key.fox_name.clone())}
                                input(size=5, bind:value=latitude)
                                input(size=5, bind:value=longitude)
                                input(type="button", value="Update", on:click=move |_|{
                                    let edit = AtomicEdit{
                                        key: postcard::to_stdvec(&key2).unwrap(),
                                        old: postcard::to_stdvec(&fox2).unwrap(),
                                        new: postcard::to_stdvec(&Fox{
                                            latitude: latitude.get().as_ref().clone(),
                                            longitude: longitude.get().as_ref().clone()
                                        }).unwrap(),
                                    };
                                    spawn_local_scoped(cx, async {queue_write.clone().send(edit).await.unwrap();});
                                })
                            }
                        }
                    },
                    key=|(key, fox)| (key.clone(), fox.clone())
                )
                input(type="button", value="Add fox!", on:click=move |_|{
                    let name = prompt("fox name", None).unwrap();
                    let edit = AtomicEdit{
                        key: postcard::to_stdvec(&Address{
                            time_slice: current_time.get().as_ref().clone(),
                            fox_name: name
                        }).unwrap(),
                        old: vec![],
                        new: postcard::to_stdvec(&Address::default()).unwrap()
                    };
                    spawn_local_scoped(cx, async {queue_write.clone().send(edit).await.unwrap();});
                })
            }
        },
        &coord_editor,
    );
}

fn main() {
    let map = &*Box::leak(Box::new(make_map()));

    // let pos = Position::new(move |p| {
    //     add_marker(map, p.coords().latitude(), p.coords().longitude());
    // });
    // forget(pos);

    let button = document()
        .get_element_by_id("add_point")
        .expect("there is a add_point button");

    let on_click = EventListener::new(&button, "click", move |_event| {
        add_marker(map, 51.5, -0.09);
    });
    on_click.forget();

    location_editor();
}
