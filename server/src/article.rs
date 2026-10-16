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
    };
    let content = article.message.content;
    state
        .db
        .transaction_mut_ok(move |txn| {
            txn.scope(|txn| {
                let new = crate::db::Article {
                    publish_at: key.publish_at.clone(),
                    article_id: key.id as i64,
                    title: value.title.clone(),
                    article_type: value.r#type.clone(),
                    content: content.clone(),
                };
                if let Err(row) = txn.insert(new) {
                    let mut article = txn.mutable(row);
                    if article.title == value.title
                        && article.article_type == value.r#type
                        && article.content == content
                    {
                        return;
                    }
                    article.title = value.title.clone();
                    article.article_type = value.r#type.clone();
                    article.content = content.clone();
                }
                let _ = state
                    .live_articles
                    .send(encode_broadcast(&key, Some(&value)));
            })
        })
        .await
}

/// The body of the article, the stream only has the title and type.
pub async fn article_content(state: &'static AppState, key: ArticleKey) -> Option<String> {
    state
        .db
        .transaction(move |txn| {
            let row = txn.query_one(
                crate::db::Article
                    .publish_at(key.publish_at)
                    .article_id(key.id as i64),
            )?;
            Some(txn.lazy(row).content.clone())
        })
        .await
}

async fn retrieve_articles_inner(state: &'static AppState) -> anyhow::Result<()> {
    let res = state.fetch("articles").await?;
    let articles: Articles = serde_json::from_str(&res)?;
    for article in articles.data {
        update_single_article(state, article).await;
    }
    Ok(())
}

pub async fn retrieve_articles_loop(state: &'static AppState) {
    loop {
        // every 5 seconds
        sleep(Duration::from_secs(5)).await;

        println!("reloading articles");
        if let Err(err) = retrieve_articles_inner(state).await {
            println!("error getting articles: {err:#}");
        }
    }
}

#[cfg(test)]
mod tests {
    use rust_query::migration::Config;

    use super::*;
    use crate::leak;

    #[tokio::test]
    async fn content_not_in_broadcast() {
        let state = leak(AppState::new(Config::open_in_memory()));
        let mut changes = state.live_articles.subscribe();

        let key = ArticleKey {
            publish_at: "2026-10-11T10:00:00+02:00".to_owned(),
            id: 1,
        };
        let article = |content: &str| Article {
            id: key.id,
            title: "Hint 1".to_owned(),
            r#type: "hint".to_owned(),
            publish_at: key.publish_at.clone(),
            message: Message {
                content: content.to_owned(),
            },
        };
        update_single_article(state, article("<p>a</p>")).await;
        update_single_article(state, article("<p>b</p>")).await;

        let value = SavedArticle {
            title: "Hint 1".to_owned(),
            r#type: "hint".to_owned(),
        };
        // a changed body is still broadcast, so clients know there is something new
        for _ in 0..2 {
            assert_eq!(
                changes.try_recv().unwrap(),
                encode_broadcast(&key, Some(&value))
            );
        }
        assert!(changes.try_recv().is_err());

        assert_eq!(
            article_content(state, key.clone()).await.unwrap(),
            "<p>b</p>"
        );
        let missing = ArticleKey { id: 2, ..key };
        assert_eq!(article_content(state, missing).await, None);
    }
}
