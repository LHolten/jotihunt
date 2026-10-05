use std::time::Duration;

use jotihunt_shared::domain::{ArticleKey, SavedArticle};
use serde::Deserialize;
use tokio::time::sleep;

use crate::{encode_broadcast, AppState};

#[derive(Deserialize)]
struct Articles {
    data: Vec<Article>,
}

#[derive(Deserialize)]
struct Article {
    id: usize,
    title: String,
    r#type: String,
    publish_at: String,
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    content: String,
}

async fn update_single_article(state: &'static AppState, article: Article) {
    let key = ArticleKey {
        publish_at: article.publish_at,
        id: article.id,
    };
    let value = SavedArticle {
        title: article.title,
        r#type: article.r#type,
        content: article.message.content,
    };
    state
        .db
        .transaction_mut_ok(move |txn| {
            txn.scope(|txn| {
                let new = crate::db::Article {
                    publish_at: key.publish_at.clone(),
                    article_id: key.id as i64,
                    title: value.title.clone(),
                    article_type: value.r#type.clone(),
                    content: value.content.clone(),
                };
                if let Err(row) = txn.insert(new) {
                    let mut article = txn.mutable(row);
                    if article.title == value.title
                        && article.article_type == value.r#type
                        && article.content == value.content
                    {
                        return;
                    }
                    article.title = value.title.clone();
                    article.article_type = value.r#type.clone();
                    article.content = value.content.clone();
                }
                let _ = state
                    .live_articles
                    .send(encode_broadcast(&key, Some(&value)));
            })
        })
        .await
}

async fn retrieve_articles_inner(state: &'static AppState) -> Result<(), reqwest::Error> {
    let res = reqwest::get("https://jotihunt.nl/api/2.0/articles").await?;
    let areas: Articles = res.json().await?;
    for area in areas.data {
        update_single_article(state, area).await;
    }
    Ok(())
}

pub async fn retrieve_articles_loop(state: &'static AppState) {
    loop {
        println!("reloading articles");
        if let Err(err) = retrieve_articles_inner(state).await {
            println!("error getting article: {err}");
        }

        // every 5 seconds
        sleep(Duration::from_secs(5)).await;
    }
}
