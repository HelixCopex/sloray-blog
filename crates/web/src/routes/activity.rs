use super::{content::actor, protection::ClientHash, session};
use crate::{error::Result, state::AppState};
use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    routing::get,
};
use serde::Deserialize;
use sloray_content::{activity_dto::*, comments, stats};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/posts/{slug}/comments", get(list).post(create))
        .route("/api/v1/posts/{id}/stats", get(get_stats))
        .route("/api/v1/posts/{id}/view", axum::routing::post(view))
        .route(
            "/api/v1/posts/{id}/like",
            axum::routing::put(like).delete(unlike),
        )
        .route("/api/v1/admin/comments", get(queue))
        .route("/api/v1/admin/comments/{id}", axum::routing::put(moderate))
}

#[derive(Deserialize)]
pub struct PageQuery {
    cursor: Option<String>,
    #[serde(default = "default_limit")]
    limit: u32,
    #[serde(default = "default_status")]
    status: String,
}
fn default_limit() -> u32 {
    20
}
fn default_status() -> String {
    "pending".into()
}

#[utoipa::path(operation_id="activity_list", get,path="/api/v1/posts/{slug}/comments",params(("slug"=String,Path),("cursor"=Option<String>,Query),("limit"=Option<u32>,Query)),responses((status=200,body=CommentPage)))]
pub async fn list(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(q): Query<PageQuery>,
) -> Result<Json<CommentPage>> {
    Ok(Json(
        comments::list(&state.db().await?, &slug, q.cursor.as_deref(), q.limit).await?,
    ))
}

#[utoipa::path(operation_id="activity_create", post,path="/api/v1/posts/{slug}/comments",params(("slug"=String,Path)),request_body=WriteComment,responses((status=202),(status=403),(status=429)))]
pub async fn create(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    Extension(hash): Extension<ClientHash>,
    Json(input): Json<WriteComment>,
) -> Result<StatusCode> {
    session::verify_csrf(&headers)?;
    let user = session::optional_user(&state, &headers).await?;
    comments::create(&state.db().await?, &slug, user, hash.0, input).await?;
    Ok(StatusCode::ACCEPTED)
}

#[utoipa::path(operation_id="activity_queue", get,path="/api/v1/admin/comments",params(("status"=Option<String>,Query),("cursor"=Option<String>,Query),("limit"=Option<u32>,Query)),responses((status=200,body=CommentPage),(status=401)))]
pub async fn queue(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<PageQuery>,
) -> Result<Json<CommentPage>> {
    let actor = actor(&state, &headers, false).await?;
    Ok(Json(
        comments::queue(
            &state.db().await?,
            actor,
            &q.status,
            q.cursor.as_deref(),
            q.limit,
        )
        .await?,
    ))
}

#[utoipa::path(operation_id="activity_moderate", put,path="/api/v1/admin/comments/{id}",params(("id"=Uuid,Path)),request_body=ModerateComment,responses((status=204),(status=401),(status=403)))]
pub async fn moderate(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(input): Json<ModerateComment>,
) -> Result<StatusCode> {
    let actor = actor(&state, &headers, true).await?;
    comments::moderate(&state.db().await?, actor, id, input).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(operation_id="activity_get_stats", get,path="/api/v1/posts/{id}/stats",params(("id"=Uuid,Path)),responses((status=200,body=Stats),(status=404)))]
pub async fn get_stats(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<Stats>> {
    let user = session::optional_user(&state, &headers).await?;
    Ok(Json(stats::get(&state.db().await?, id, user).await?))
}

#[utoipa::path(operation_id="activity_view", post,path="/api/v1/posts/{id}/view",params(("id"=Uuid,Path)),responses((status=204),(status=403),(status=429)))]
pub async fn view(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Extension(hash): Extension<ClientHash>,
) -> Result<StatusCode> {
    session::verify_csrf(&headers)?;
    let agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let visitor = sloray_auth::token_hash(&format!(
        "{}:{}",
        hex::encode(hash.0),
        agent.chars().take(512).collect::<String>()
    ));
    stats::view(&state.db().await?, id, visitor).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(operation_id="activity_like", put,path="/api/v1/posts/{id}/like",params(("id"=Uuid,Path)),responses((status=204),(status=401),(status=429)))]
pub async fn like(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<StatusCode> {
    change_like(state, id, headers, true).await
}
#[utoipa::path(operation_id="activity_unlike", delete,path="/api/v1/posts/{id}/like",params(("id"=Uuid,Path)),responses((status=204),(status=401),(status=429)))]
pub async fn unlike(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<StatusCode> {
    change_like(state, id, headers, false).await
}
async fn change_like(
    state: AppState,
    id: Uuid,
    headers: HeaderMap,
    liked: bool,
) -> Result<StatusCode> {
    let actor = actor(&state, &headers, true).await?;
    stats::like(&state.db().await?, id, actor.id, liked).await?;
    Ok(StatusCode::NO_CONTENT)
}
