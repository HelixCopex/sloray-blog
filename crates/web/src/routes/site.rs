use crate::state::AppState;
use axum::{Json, extract::State};
use serde::Serialize;
use utoipa::ToSchema;
#[derive(Serialize, ToSchema)]
pub struct Site {
    public_site_url: Option<String>,
    identity: sloray_content::site::SiteIdentity,
}
#[utoipa::path(operation_id="site_site", get,path="/api/v1/site",responses((status=200,body=Site)))]
pub async fn site(State(state): State<AppState>) -> Json<Site> {
    let identity = match state.db_optional().await {
        Some(db) => sloray_content::site::get(&db).await.unwrap_or_default(),
        None => Default::default(),
    };
    Json(Site {
        public_site_url: state.config().await.public_site_url,
        identity,
    })
}
