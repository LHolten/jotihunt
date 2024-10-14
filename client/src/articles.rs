use gloo::utils::document;
use jotihunt_shared::domain::{ArticleKey, SavedArticle};
use sycamore::{
    builder::tag,
    flow::Keyed,
    prelude::{create_memo, create_ref, create_signal},
    view,
};
use web_sys::Element;

use crate::comms::live_updated;

pub fn articles(key: &'static str) {
    let articles = document()
        .get_element_by_id("articles")
        .expect("there is an articles element");

    sycamore::render_to(
        |cx| {
            let (articles, _) = live_updated::<ArticleKey, SavedArticle>(cx, key, "articles");

            let everything = create_signal(cx, false);

            let filtered = create_memo(cx, || {
                articles
                    .get()
                    .iter()
                    .filter_map(|(k, v)| {
                        (*everything.get() || &v.r#type == "hint").then_some((k.clone(), v.clone()))
                    })
                    .collect::<Vec<_>>()
            });

            view! {cx,
                summary {"Hints"}
                div(class="field") {
                    label(for="everything"){"Alles: "}
                    input(type="checkbox", id="everything", bind:checked=everything)
                }
                Keyed(
                    iterable=filtered,
                    view=move|cx, (key, article)| {
                        let article = create_ref(cx, article);
                        view!{cx,
                            p {
                                input (type="button", on:click = move |_| {
                                    update_page(&key, article);
                                }, value=(article.title))
                            }
                        }
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

fn update_page(key: &ArticleKey, article: &SavedArticle) {
    let (key, article) = (key.clone(), article.clone());

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
                time {(key.publish_at)}
                p {(content)}
            }
        },
        &page,
    )
}
