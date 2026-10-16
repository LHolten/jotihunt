use std::{cell::Cell, collections::BTreeMap};

use gloo::{
    dialogs::alert,
    net::http::Request,
    utils::{document, window},
};
use jotihunt_shared::domain::{ArticleKey, SavedArticle, StatusKey};
use js_sys::encode_uri_component;
use merging_iterator::MergeIter;
use sycamore::{futures::spawn_local_scoped, prelude::*};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::Element;

use crate::{comms::live_table, leaflet, HOSTNAME};

#[derive(Clone, PartialEq, Eq, Hash)]
enum Update {
    Status {
        area: String,
        status: String,
        until: Option<String>,
    },
    Article(ArticleKey, SavedArticle),
}

impl Update {
    fn view(self, time: String) -> View {
        let date = js_sys::Date::new_0();
        date.set_time(js_sys::Date::parse(&time));
        let hours = date.get_hours();
        let mins = date.get_minutes();
        let time_short = format!("{hours:0>2}:{mins:0>2}");
        match self {
            Update::Status {
                area,
                status,
                until,
            } => {
                let extra = until
                    .map(|x| {
                        let diff = js_sys::Date::parse(&x) - js_sys::Date::parse(&time);
                        let mins = (diff / 1000. / 60.) as u32;
                        format!(" voor {mins}min")
                    })
                    .unwrap_or_default();
                let status = match &*status {
                    "green" => "groen".to_owned(),
                    "orange" => "oranje".to_owned(),
                    "red" => "rood".to_owned(),
                    _ => status,
                };
                view! {
                    p {
                        time(datetime=time) {(time_short)} " "
                        (area) " is " (status) (extra)
                    }
                }
            }
            Update::Article(key, article) => {
                let title = format!("{time_short}: {}", article.title);
                view! {
                    p {
                        input (r#type="button", on:click = move |_| {
                            update_page(&key, &article);
                        }, value=title)
                    }
                }
            }
        }
    }

    fn kind(&self) -> &str {
        match self {
            Update::Status { .. } => "nieuwe status",
            Update::Article(_, saved_article) => match &*saved_article.r#type {
                "hint" => "nieuwe hint",
                "assignment" => "nieuwe opdracht",
                "news" => "nieuw bericht",
                x => x,
            },
        }
    }
}

pub fn articles() {
    let articles = document()
        .get_element_by_id("articles")
        .expect("there is an articles element");

    sycamore::render_to(
        || {
            let articles =
                live_table::<ArticleKey, SavedArticle>(format!("https://{HOSTNAME}/articles"));
            let status = live_table::<StatusKey, String>(format!("https://{HOSTNAME}/status"));

            // the legend shows the current status of each area, since it last changed
            create_effect(move || {
                status.with(|status| {
                    let mut current = BTreeMap::<&str, (&str, &str)>::new();
                    for (key, value) in status {
                        // the status is polled, so it can repeat without changing
                        if current.get(&*key.fox_name).map(|(old, _)| *old) != Some(value.as_str())
                        {
                            current.insert(&key.fox_name, (value, &key.date_time));
                        }
                    }
                    for (area, (value, since)) in current {
                        leaflet::set_area_status(area, value, since);
                    }
                })
            });

            let status_check = create_signal(false);
            let everything = create_signal(false);

            let combined = create_memo(move || {
                let left: Vec<_> = articles.with(|articles| {
                    articles
                        .iter()
                        .filter(|(_, v)| everything.get() || &v.r#type == "hint")
                        .map(|(k, v)| (k.publish_at.clone(), Update::Article(k.clone(), v.clone())))
                        .collect()
                });
                let right: Vec<_> = status.with(|status| {
                    status
                        .iter()
                        .filter(|(_, v)| status_check.get() && *v != "green")
                        .map(|(k, v)| {
                            (
                                k.date_time.clone(),
                                Update::Status {
                                    area: k.fox_name.clone(),
                                    status: v.clone(),
                                    until: status
                                        .range(k..)
                                        .skip(1)
                                        .find(|(k2, _)| k2.fox_name == k.fox_name)
                                        .map(|(k2, _)| k2.date_time.clone()),
                                },
                            )
                        })
                        .collect()
                });

                let mut res =
                    MergeIter::with_custom_ordering(left, right, |a, b| a.0.cmp(&b.0).is_lt())
                        .collect::<Vec<_>>();
                res.reverse();
                res.dedup();
                res
            });

            let last_updated = Cell::new(js_sys::Date::now());
            create_effect(move || {
                if let Some((time, item)) = combined.with(|combined| combined.first().cloned()) {
                    let most_recent = js_sys::Date::parse(&time);
                    if most_recent > last_updated.get() {
                        last_updated.set(most_recent);
                        notify(&item);
                    }
                }
            });

            view! {
                summary(title="Tijdlijn") { span(class="icon") {"📰"} span(class="label") {" Tijdlijn"} }
                div(class="field") {
                    label(r#for="everything"){"Berichten: "}
                    input(r#type="checkbox", id="everything", bind:checked=everything)

                    label(r#for="status_check"){"Status: "}
                    input(r#type="checkbox", id="status_check", bind:checked=status_check)
                }
                Keyed(
                    list=combined,
                    view=|(time, update)| update.view(time),
                    key=|x|x.clone()
                )
            }
        },
        &articles,
    );
}

fn get_element(name: &str) -> Element {
    document()
        .get_element_by_id(name)
        .expect(&format!("there is a {name} element"))
}

fn reset_page() {
    let page = get_element("page");
    let map = get_element("map");
    let panel_column = get_element("panel_column");

    page.set_attribute("hidden", "").unwrap();
    map.remove_attribute("hidden").unwrap();
    panel_column.remove_attribute("hidden").unwrap();
}

/// The stream does not include the body of articles, so it is loaded here.
async fn article_content(key: &ArticleKey) -> String {
    let publish_at = String::from(encode_uri_component(&key.publish_at));
    let address = format!("https://{HOSTNAME}/articles/{publish_at}/{}", key.id);
    let res = Request::get(&address).send().await.unwrap();
    res.text().await.unwrap()
}

fn update_page(key: &ArticleKey, article: &SavedArticle) {
    let key = key.clone();
    let article = article.clone();

    let page = get_element("page");
    let map = get_element("map");
    let panel_column = get_element("panel_column");

    page.remove_attribute("hidden").unwrap();
    map.set_attribute("hidden", "").unwrap();
    panel_column.set_attribute("hidden", "").unwrap();

    page.set_inner_html("");

    sycamore::render_to(
        move || {
            let content = create_node_ref();
            spawn_local_scoped(async move {
                let html = article_content(&key).await;
                content
                    .get()
                    .unchecked_into::<Element>()
                    .set_inner_html(&html);
            });

            view! {
                input(r#type="button", value="Terug", on:click=|_|{
                    reset_page()
                })
                h1 {(article.title)}
                p {div(ref=content) {"laden..."}}
            }
        },
        &page,
    )
}

fn notify(item: &Update) {
    let kind = item.kind();
    let _ = try_speak(&format!("{kind}"));
    alert(&format!("Er is een {kind} op de tijdlijn!"));
}

fn try_speak(text: &str) -> Result<(), JsValue> {
    let speech = window().speech_synthesis()?;
    let utter = web_sys::SpeechSynthesisUtterance::new_with_text(text)?;
    utter.set_lang("nl");
    speech.speak(&utter);
    Ok(())
}
