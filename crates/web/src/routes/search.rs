use crate::{error::Result, state::AppState};
use axum::{
    Json,
    extract::{Query, State},
};
use serde::Deserialize;
use sloray_content::search::{PostgresSearch, SearchIndex, SearchPage};

#[derive(Deserialize)]
pub struct SearchQuery {
    q: String,
    cursor: Option<String>,
    #[serde(default = "default_limit")]
    limit: u32,
}
fn default_limit() -> u32 {
    20
}

#[utoipa::path(operation_id="search_search", get,path="/api/v1/search",params(("q"=String,Query),("cursor"=Option<String>,Query),("limit"=Option<u32>,Query)),responses((status=200,body=SearchPage),(status=422)))]
pub async fn search(
    State(state): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<SearchPage>> {
    Ok(Json(
        PostgresSearch {
            db: &state.db().await?,
        }
        .search(&q.q, q.cursor.as_deref(), q.limit)
        .await?,
    ))
}
