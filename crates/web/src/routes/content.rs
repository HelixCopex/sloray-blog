//! 文章 API 编排；校验、授权与事务交给 content。
use crate::{AppError, error::Result, state::AppState};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use sloray_content::{Actor, dto::*, posts, revisions};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/posts", get(list_posts))
        .route("/api/v1/posts/{slug}", get(get_post))
        .route("/api/v1/tags", get(list_tags))
        .route("/api/v1/admin/posts", get(admin_posts).post(create_post))
        .route(
            "/api/v1/admin/posts/{id}",
            get(admin_post).put(update_post).delete(delete_post),
        )
        .route("/api/v1/admin/posts/{id}/revisions", get(list_revisions))
}

#[derive(Deserialize)]
pub struct ListQuery {
    cursor: Option<String>,
    tag: Option<String>,
    #[serde(default = "default_limit")]
    limit: u32,
}
fn default_limit() -> u32 {
    20
}

#[utoipa::path(operation_id="content_list_posts", get, path="/api/v1/posts", params(("cursor"=Option<String>, Query), ("tag"=Option<String>, Query), ("limit"=Option<u32>, Query)), responses((status=200, body=PostPage)))]
pub async fn list_posts(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<PostPage>> {
    Ok(Json(
        posts::list(
            &state.db().await?,
            query.cursor.as_deref(),
            query.tag.as_deref(),
            query.limit,
        )
        .await?,
    ))
}

#[utoipa::path(operation_id="content_get_post", get, path="/api/v1/posts/{slug}", params(("slug"=String, Path)), responses((status=200, body=Post), (status=304), (status=404)))]
pub async fn get_post(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Result<Response> {
    let post = posts::by_slug(&state.db().await?, &slug).await?;
    let representation = serde_json::to_string(&post).map_err(|e| AppError::Internal(e.into()))?;
    let etag = format!(
        "\"{}\"",
        hex::encode(sloray_auth::token_hash(&representation))
    );
    let matches = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|h| {
            h.split(',')
                .any(|v| v.trim().trim_start_matches("W/") == etag || v.trim() == "*")
        });
    let mut response = if matches {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        Json(post).into_response()
    };
    response.headers_mut().insert(
        header::ETAG,
        etag.parse()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("etag: {e}")))?,
    );
    // 必须重验证，撤回或删除不会残留已缓存的正文；共享缓存仅在 purge 实现后启用。
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        "public, no-cache".parse().expect("static header"),
    );
    Ok(response)
}

#[utoipa::path(operation_id="content_list_tags", get, path="/api/v1/tags", responses((status=200, body=Vec<Tag>)))]
pub async fn list_tags(State(state): State<AppState>) -> Result<Json<Vec<Tag>>> {
    Ok(Json(posts::tags(&state.db().await?).await?))
}

pub(super) async fn actor(state: &AppState, headers: &HeaderMap, writing: bool) -> Result<Actor> {
    let token = sloray_auth::read_cookie(headers, sloray_auth::SESSION_COOKIE)
        .ok_or(AppError::Unauthorized)?;
    let user = sloray_auth::user_for_session(&state.db().await?, &token)
        .await?
        .ok_or(AppError::Unauthorized)?;
    if writing {
        super::session::verify_csrf(headers)?;
    }
    Ok(Actor {
        id: user.id,
        admin: user.is_admin(),
    })
}

#[utoipa::path(operation_id="content_admin_posts", get, path="/api/v1/admin/posts", params(("cursor"=Option<String>,Query),("limit"=Option<u32>,Query)), responses((status=200, body=PostPage), (status=401)))]
pub async fn admin_posts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<Json<PostPage>> {
    let actor = actor(&state, &headers, false).await?;
    Ok(Json(
        posts::admin_list(&state.db().await?, actor, q.cursor.as_deref(), q.limit).await?,
    ))
}

#[utoipa::path(operation_id="content_admin_post", get, path="/api/v1/admin/posts/{id}", params(("id"=Uuid,Path)), responses((status=200,body=Post), (status=401), (status=403)))]
pub async fn admin_post(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<Post>> {
    let actor = actor(&state, &headers, false).await?;
    Ok(Json(posts::owned(&state.db().await?, id, actor).await?))
}

#[utoipa::path(operation_id="content_create_post", post, path="/api/v1/admin/posts", request_body=WritePost, responses((status=201,body=Post), (status=401), (status=403), (status=422)))]
pub async fn create_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<WritePost>,
) -> Result<(StatusCode, Json<Post>)> {
    let actor = actor(&state, &headers, true).await?;
    Ok((
        StatusCode::CREATED,
        Json(posts::save(&state.db().await?, actor, None, input).await?),
    ))
}

#[utoipa::path(operation_id="content_update_post", put, path="/api/v1/admin/posts/{id}", params(("id"=Uuid,Path)), request_body=WritePost, responses((status=200,body=Post), (status=409), (status=401), (status=403)))]
pub async fn update_post(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(input): Json<WritePost>,
) -> Result<Json<Post>> {
    let actor = actor(&state, &headers, true).await?;
    Ok(Json(
        posts::save(&state.db().await?, actor, Some(id), input).await?,
    ))
}

#[utoipa::path(operation_id="content_delete_post", delete, path="/api/v1/admin/posts/{id}", params(("id"=Uuid,Path)), request_body=DeletePost, responses((status=204), (status=409), (status=401), (status=403)))]
pub async fn delete_post(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(input): Json<DeletePost>,
) -> Result<StatusCode> {
    let actor = actor(&state, &headers, true).await?;
    posts::delete(&state.db().await?, actor, id, input.version).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(operation_id="content_list_revisions", get, path="/api/v1/admin/posts/{id}/revisions", params(("id"=Uuid,Path),("cursor"=Option<String>,Query),("limit"=Option<u32>,Query)), responses((status=200,body=RevisionPage), (status=401), (status=403)))]
pub async fn list_revisions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<Json<RevisionPage>> {
    let actor = actor(&state, &headers, false).await?;
    Ok(Json(
        revisions::list(&state.db().await?, actor, id, q.cursor.as_deref(), q.limit).await?,
    ))
}
