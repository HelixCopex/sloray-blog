//! 主题 API 只负责鉴权与 HTTP 编排。
use crate::{error::Result, state::AppState};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, put},
};
use sloray_content::{dto::DeletePost, theme_dto::*, themes};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/theme", get(public))
        .route("/api/v1/admin/themes", get(list).post(create))
        .route("/api/v1/admin/themes/active", put(activate))
        .route("/api/v1/admin/themes/{id}", put(update).delete(delete))
}
#[utoipa::path(operation_id="themes_public", get, path="/api/v1/theme", responses((status=200,body=ThemeState)))]
pub async fn public(State(state): State<AppState>) -> Result<Json<ThemeState>> {
    Ok(Json(themes::state(&state.db().await?).await?))
}
#[utoipa::path(operation_id="themes_list", get, path="/api/v1/admin/themes", responses((status=200,body=Vec<Theme>)))]
pub async fn list(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<Vec<Theme>>> {
    let actor = super::content::actor(&state, &headers, false).await?;
    Ok(Json(themes::list(&state.db().await?, actor).await?))
}
#[utoipa::path(operation_id="themes_create", post, path="/api/v1/admin/themes", request_body=WriteTheme, responses((status=201,body=Theme)))]
pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<WriteTheme>,
) -> Result<(StatusCode, Json<Theme>)> {
    let actor = super::content::actor(&state, &headers, true).await?;
    Ok((
        StatusCode::CREATED,
        Json(themes::save(&state.db().await?, actor, None, input).await?),
    ))
}
#[utoipa::path(operation_id="themes_update", put, path="/api/v1/admin/themes/{id}", params(("id"=Uuid,Path)), request_body=WriteTheme, responses((status=200,body=Theme)))]
pub async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<WriteTheme>,
) -> Result<Json<Theme>> {
    let actor = super::content::actor(&state, &headers, true).await?;
    Ok(Json(
        themes::save(&state.db().await?, actor, Some(id), input).await?,
    ))
}
#[utoipa::path(operation_id="themes_activate", put, path="/api/v1/admin/themes/active", request_body=ActivateTheme, responses((status=200,body=ThemeState)))]
pub async fn activate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ActivateTheme>,
) -> Result<Json<ThemeState>> {
    let actor = super::content::actor(&state, &headers, true).await?;
    Ok(Json(
        themes::activate(&state.db().await?, actor, input).await?,
    ))
}
#[utoipa::path(operation_id="themes_delete", delete, path="/api/v1/admin/themes/{id}", params(("id"=Uuid,Path)), request_body=DeletePost, responses((status=204)))]
pub async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<DeletePost>,
) -> Result<StatusCode> {
    let actor = super::content::actor(&state, &headers, true).await?;
    themes::delete(&state.db().await?, actor, id, input.version).await?;
    Ok(StatusCode::NO_CONTENT)
}
