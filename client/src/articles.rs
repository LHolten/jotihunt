use std::cell::Cell;

use gloo::{
    dialogs::alert,
    utils::{document, window},
};
use jotihunt_shared::domain::{ArticleKey, SavedArticle, StatusKey};
use merging_iterator::MergeIter;
use sycamore::{
    builder::tag,
    flow::Keyed,
    prelude::{create_effect, create_memo, create_ref, create_signal, BoundedScope},
    view,
    web::DomNode,
};
use wasm_bindgen::JsValue;
use web_sys::Element;

use crate::comms::live_updated;

#[derive(Clone, PartialEq, Eq, Hash)]
enum Update {
    Status {
        area: String,
        status: String,
        until: Option<String>,
    },
    Article(SavedArticle),
}

impl Update {
    fn view<'cx>(self, time: String, cx: BoundedScope<'cx, 'cx>) -> view::View<DomNode> {
        let time_short = time.get(11..16).unwrap_or("").to_owned();
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
                view! {cx,
                    p {
                        time(datetime=time) {(time_short)} " "
                        (area) " is " (status) (extra)
                    }
                }
            }
            Update::Article(article) => {
                let title = article.title.clone();
                view! {cx,
                    p {
                        input (type="button", on:click = move |_| {
                            update_page(&time, &article);
                        }, value=(title))
                    }
                }
            }
        }
    }

    fn kind(&self) -> &str {
        match self {
            Update::Status { .. } => "nieuwe status",
            Update::Article(saved_article) => match &*saved_article.r#type {
                "hint" => "nieuwe hint",
                "assignment" => "nieuwe opdracht",
                "news" => "nieuw bericht",
                x => x,
            },
        }
    }
}

pub fn articles(key: &'static str) {
    let articles = document()
        .get_element_by_id("articles")
        .expect("there is an articles element");

    sycamore::render_to(
        |cx| {
            let (articles, _) = live_updated::<ArticleKey, SavedArticle>(cx, key, "articles");
            let (status, _) = live_updated::<StatusKey, String>(cx, key, "status");

            let status_check = create_signal(cx, false);
            let everything = create_signal(cx, false);

            let combined = create_memo(cx, || {
                let get = articles.get();
                let left = get
                    .iter()
                    .filter(|(_, v)| *everything.get() || &v.r#type == "hint")
                    .map(|(k, v)| (k.publish_at.clone(), Update::Article(v.clone())));
                let get = status.get();
                let right = get
                    .iter()
                    .filter(|(_, v)| *status_check.get() && *v != "green")
                    .map(|(k, v)| {
                        (
                            k.date_time.clone(),
                            Update::Status {
                                area: k.fox_name.clone(),
                                status: v.clone(),
                                until: get
                                    .range(k..)
                                    .skip(1)
                                    .find(|(k2, _)| k2.fox_name == k.fox_name)
                                    .map(|(k2, _)| k2.date_time.clone()),
                            },
                        )
                    });

                let mut res =
                    MergeIter::with_custom_ordering(left, right, |a, b| a.0.cmp(&b.0).is_lt())
                        .collect::<Vec<_>>();
                res.reverse();
                res
            });

            let last_updated = create_ref(cx, Cell::new(js_sys::Date::now()));
            create_effect(cx, || {
                if let Some((time, item)) = combined.get().last() {
                    let most_recent = js_sys::Date::parse(time);
                    if most_recent > last_updated.get() {
                        last_updated.set(most_recent);
                        notify(item);
                    }
                }
            });

            view! {cx,
                summary {"Tijdlijn"}
                div(class="field") {
                    label(for="everything"){"Berichten: "}
                    input(type="checkbox", id="everything", bind:checked=everything)

                    label(for="status_check"){"Status: "}
                    input(type="checkbox", id="status_check", bind:checked=status_check)
                }
                Keyed(
                    iterable=combined,
                    view=move|cx, (time, update)| {
                        update.view(time, cx)
                    },
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

fn update_page(time: &String, article: &SavedArticle) {
    let (time, article) = (time.clone(), article.clone());

    let page = get_element("page");
    let map = get_element("map");
    let panel_column = get_element("panel_column");

    page.remove_attribute("hidden").unwrap();
    map.set_attribute("hidden", "").unwrap();
    panel_column.set_attribute("hidden", "").unwrap();

    page.set_inner_html("");

    sycamore::render_to(
        |cx| {
            let content = tag("div")
                .dangerously_set_inner_html(article.content)
                .view(cx);

            view! {cx,
                input(type="button", value="Terug", on:click=|_|{
                    reset_page()
                })
                h1 {(article.title)}
                time {(time)}
                p {(content)}
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
