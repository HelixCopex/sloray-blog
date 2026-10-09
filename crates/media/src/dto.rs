use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Deserialize, ToSchema)]
pub struct PresignInput {
    pub content_type: String,
    pub size_bytes: i64,
    pub sha256: String,
}
#[derive(Serialize, ToSchema)]
pub struct Presigned {
    pub upload_id: Option<Uuid>,
    pub upload_url: Option<String>,
    pub headers: std::collections::BTreeMap<String, String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub media: Option<Media>,
}
#[derive(Deserialize, ToSchema)]
pub struct RegisterInput {
    pub upload_id: Uuid,
}
#[derive(Serialize, ToSchema)]
pub struct Variant {
    pub width: i32,
    pub url: String,
}
#[derive(Serialize, ToSchema)]
pub struct Media {
    pub id: Uuid,
    pub mime: String,
    pub size_bytes: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub ready: bool,
    pub variants: Vec<Variant>,
    pub created_at: DateTime<Utc>,
}
