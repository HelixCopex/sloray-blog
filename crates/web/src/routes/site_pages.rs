//! 页面 HTTP 编排；权限、校验和版本控制在 content 内。
use crate::{error::Result, state::AppState};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    routing::get,
};
use serde::Deserialize;
use sloray_content::{dto::DeletePost, page_dto::*, pages};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/pages", get(list))
        .route("/api/v1/pages/{slug}", get(public_page))
        .route("/api/v1/admin/pages", get(admin_list).post(create))
        .route(
            "/api/v1/admin/pages/{id}",
            get(detail).put(update).delete(delete),
        )
}

#[derive(Deserialize)]
pub struct ListQuery {
    cursor: Option<String>,
    limit: Option<u32>,
}

#[utoipa::path(operation_id="pages_list", get, path="/api/v1/pages", params(("cursor"=Option<String>,Query),("limit"=Option<u32>,Query)), responses((status=200,body=PageList)))]
pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<PageList>> {
    Ok(Json(
        pages::list(
            &state.db().await?,
            true,
            q.cursor.as_deref(),
            q.limit.unwrap_or(20),
        )
        .await?,
    ))
}

#[utoipa::path(operation_id="pages_public_page", get, path="/api/v1/pages/{slug}", params(("slug"=String,Path)), responses((status=200,body=Page),(status=404)))]
pub async fn public_page(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Page>> {
    Ok(Json(pages::by_slug(&state.db().await?, &slug).await?))
}

#[utoipa::path(operation_id="pages_admin_list", get, path="/api/v1/admin/pages", params(("cursor"=Option<String>,Query),("limit"=Option<u32>,Query)), responses((status=200,body=PageList)))]
pub async fn admin_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<Json<PageList>> {
    let actor = super::content::actor(&state, &headers, false).await?;
    Ok(Json(
        pages::admin_list(
            &state.db().await?,
            actor,
            q.cursor.as_deref(),
            q.limit.unwrap_or(20),
        )
        .await?,
    ))
}

#[utoipa::path(operation_id="pages_detail", get, path="/api/v1/admin/pages/{id}", params(("id"=Uuid,Path)), responses((status=200,body=Page)))]
pub async fn detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Page>> {
    let actor = super::content::actor(&state, &headers, false).await?;
    Ok(Json(pages::get(&state.db().await?, actor, id).await?))
}

#[utoipa::path(operation_id="pages_create", post, path="/api/v1/admin/pages", request_body=WritePage, responses((status=201,body=Page)))]
pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<WritePage>,
) -> Result<(StatusCode, Json<Page>)> {
    let actor = super::content::actor(&state, &headers, true).await?;
    Ok((
        StatusCode::CREATED,
        Json(pages::save(&state.db().await?, actor, None, input).await?),
    ))
}

#[utoipa::path(operation_id="pages_update", put, path="/api/v1/admin/pages/{id}", params(("id"=Uuid,Path)), request_body=WritePage, responses((status=200,body=Page)))]
pub async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<WritePage>,
) -> Result<Json<Page>> {
    let actor = super::content::actor(&state, &headers, true).await?;
    Ok(Json(
        pages::save(&state.db().await?, actor, Some(id), input).await?,
    ))
}

#[utoipa::path(operation_id="pages_delete", delete, path="/api/v1/admin/pages/{id}", params(("id"=Uuid,Path)), request_body=DeletePost, responses((status=204)))]
pub async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<DeletePost>,
) -> Result<StatusCode> {
    let actor = super::content::actor(&state, &headers, true).await?;
    pages::delete(&state.db().await?, actor, id, input.version).await?;
    Ok(StatusCode::NO_CONTENT)
}
