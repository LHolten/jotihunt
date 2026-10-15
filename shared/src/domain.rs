use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FoxKey {
    pub day: String,
    pub time: String,
    pub fox_name: String,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fox {
    pub latitude: String,
    pub longitude: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StatusKey {
    pub date_time: String,
    pub fox_name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArticleKey {
    pub publish_at: String,
    pub id: usize,
}

/// The body is not included, it is at `GET /articles/{publish_at}/{id}`.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SavedArticle {
    pub title: String,
    pub r#type: String,
}

/// An element of `GET /areas.json`, in the order the client should use for the colors.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Area {
    pub name: String,
    /// `false` for the areas we are not allowed to hunt, listed in the `excluded_areas` file.
    pub huntable: bool,
}
