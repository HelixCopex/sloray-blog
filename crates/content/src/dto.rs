use chrono::{DateTime, Utc};
use sea_orm::FromQueryResult;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, FromQueryResult)]
pub struct Tag {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, FromQueryResult)]
pub struct PostSummary {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub published_at: Option<DateTime<Utc>>,
    pub author_id: Uuid,
    pub author_name: String,
    pub version: i32,
    pub updated_at: DateTime<Utc>,
    pub status: String,
    pub view_count: i64,
    pub like_count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PostPage {
    pub items: Vec<PostSummary>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Serialize, ToSchema, FromQueryResult)]
pub struct Post {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub body_md: String,
    pub status: String,
    pub published_at: Option<DateTime<Utc>>,
    pub author_id: Uuid,
    pub author_name: String,
    pub cover_media_id: Option<Uuid>,
    pub lang: String,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[sea_orm(skip)]
    pub tags: Vec<Tag>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct WritePost {
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub summary: String,
    pub body_md: String,
    pub status: String,
    pub published_at: Option<DateTime<Utc>>,
    pub cover_media_id: Option<Uuid>,
    #[serde(default = "default_lang")]
    pub lang: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// 更新必须提供当前版本；新建时不需要。
    pub version: Option<i32>,
}
fn default_lang() -> String {
    "zh".to_owned()
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DeletePost {
    pub version: i32,
}

#[derive(Debug, Serialize, ToSchema, FromQueryResult)]
pub struct Revision {
    pub id: Uuid,
    pub post_id: Uuid,
    pub title: String,
    pub body_md: String,
    pub editor_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RevisionPage {
    pub items: Vec<Revision>,
    pub next_cursor: Option<String>,
}
