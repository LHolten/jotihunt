use std::time::Duration;

use jotihunt_shared::domain::SavedArticle;
use serde::Deserialize;
use sled::Db;
use tokio::time::sleep;

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

fn update_single_article(tree: &sled::Tree, article: Article) -> Result<(), postcard::Error> {
    let key = postcard::to_allocvec(&(&article.publish_at, &article.id))?;
    let value = postcard::to_allocvec(&SavedArticle {
        title: article.title,
        r#type: article.r#type,
        content: article.message.content,
    })?;
    if tree.get(&key).unwrap().as_slice() != Some(&value).as_slice() {
        let _old = tree.insert(&key, value).unwrap();
    }
    Ok(())
}

async fn retrieve_articles_inner(tree: &sled::Tree) -> Result<(), reqwest::Error> {
    let res = reqwest::get("https://jotihunt.nl/api/2.0/articles").await?;
    let areas: Articles = res.json().await?;
    for area in areas.data {
        if let Err(err) = update_single_article(tree, area) {
            println!("error handling article: {err}")
        }
    }
    Ok(())
}

pub async fn retrieve_articles_loop(db: &Db) {
    let tree = db.open_tree("articles").unwrap();

    loop {
        println!("reloading articles");
        if let Err(err) = retrieve_articles_inner(&tree).await {
            println!("error getting article: {err}");
        }

        // every 5 seconds
        sleep(Duration::from_secs(5)).await;
    }
}
