use chrono::{DateTime, Utc};
use sea_orm::FromQueryResult;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Serialize, ToSchema, FromQueryResult)]
pub struct PageSummary {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub status: String,
    pub show_in_nav: bool,
    pub nav_order: i32,
    pub dock_icon: String,
    pub version: i32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema, FromQueryResult)]
pub struct Page {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub body_md: String,
    pub status: String,
    pub show_in_nav: bool,
    pub nav_order: i32,
    pub dock_icon: String,
    pub version: i32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PageList {
    pub items: Vec<PageSummary>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct WritePage {
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub summary: String,
    pub body_md: String,
    pub status: String,
    pub show_in_nav: bool,
    pub nav_order: i32,
    #[serde(default = "default_dock_icon")]
    pub dock_icon: String,
    pub version: Option<i32>,
}
fn default_dock_icon() -> String {
    "article".into()
}
