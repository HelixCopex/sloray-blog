use chrono::{DateTime, Utc};
use sea_orm::FromQueryResult;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Serialize, ToSchema, FromQueryResult)]
pub struct Comment {
    pub id: Uuid,
    pub post_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub author_name: String,
    pub body_md: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CommentPage {
    pub items: Vec<Comment>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct WriteComment {
    pub guest_name: String,
    pub body_md: String,
    pub parent_id: Option<Uuid>,
    /// 蜜罐，正常访客必须留空。
    #[serde(default)]
    pub website: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ModerateComment {
    pub status: String,
}

#[derive(Debug, Serialize, ToSchema, FromQueryResult)]
pub struct Stats {
    pub view_count: i64,
    pub like_count: i64,
    pub liked: bool,
}
