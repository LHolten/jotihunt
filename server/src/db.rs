use jotihunt_shared::domain::{ArticleKey, Fox, FoxKey, SavedArticle, StatusKey};
use rust_query::{migration::schema, FromExpr};

#[schema(Schema)]
pub mod vN {
    #[no_reference]
    #[unique(day, time, fox_name)]
    pub struct Location {
        pub day: String,
        pub time: String,
        pub fox_name: String,
        pub latitude: String,
        pub longitude: String,
    }
    #[no_reference]
    #[unique(date_time, fox_name)]
    pub struct Status {
        pub date_time: String,
        pub fox_name: String,
        pub status: String,
    }
    #[no_reference]
    #[unique(publish_at, article_id)]
    pub struct Article {
        pub publish_at: String,
        pub article_id: i64,
        pub title: String,
        pub article_type: String,
        pub content: String,
    }
}
pub use v0::*;

#[derive(FromExpr)]
#[rust_query(From = Location)]
pub struct LocationRow {
    pub day: String,
    pub time: String,
    pub fox_name: String,
    pub latitude: String,
    pub longitude: String,
}

impl LocationRow {
    pub fn into_parts(self) -> (FoxKey, Fox) {
        let key = FoxKey {
            day: self.day,
            time: self.time,
            fox_name: self.fox_name,
        };
        let fox = Fox {
            latitude: self.latitude,
            longitude: self.longitude,
        };
        (key, fox)
    }
}

#[derive(FromExpr)]
#[rust_query(From = Status)]
pub struct StatusRow {
    pub date_time: String,
    pub fox_name: String,
    pub status: String,
}

impl StatusRow {
    pub fn into_parts(self) -> (StatusKey, String) {
        let key = StatusKey {
            date_time: self.date_time,
            fox_name: self.fox_name,
        };
        (key, self.status)
    }
}

#[derive(FromExpr)]
#[rust_query(From = Article)]
pub struct ArticleRow {
    pub publish_at: String,
    pub article_id: i64,
    pub title: String,
    pub article_type: String,
    pub content: String,
}

impl ArticleRow {
    pub fn into_parts(self) -> (ArticleKey, SavedArticle) {
        let key = ArticleKey {
            publish_at: self.publish_at,
            id: self.article_id as usize,
        };
        let article = SavedArticle {
            title: self.title,
            r#type: self.article_type,
            content: self.content,
        };
        (key, article)
    }
}
