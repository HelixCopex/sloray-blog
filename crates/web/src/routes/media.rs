use crate::{AppError, error::Result, state::AppState};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::get,
};
use sloray_media::{dto::*, uploads};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/admin/media", get(list).post(register))
        .route("/api/v1/admin/media/presign", axum::routing::post(presign))
        .route("/api/v1/admin/media/{id}", get(get_media).delete(delete))
        .route("/api/v1/media/{id}", get(public_media))
}
async fn admin(state: &AppState, headers: &HeaderMap, writing: bool) -> Result<Uuid> {
    let actor = super::content::actor(state, headers, writing).await?;
    if !actor.admin {
        return Err(AppError::Forbidden);
    }
    Ok(actor.id)
}
#[utoipa::path(operation_id="media_list", get,path="/api/v1/admin/media",responses((status=200,body=Vec<Media>),(status=403)))]
pub async fn list(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<Vec<Media>>> {
    admin(&state, &headers, false).await?;
    Ok(Json(
        uploads::list(&state.db().await?, &state.config().await).await?,
    ))
}
#[utoipa::path(operation_id="media_presign", post,path="/api/v1/admin/media/presign",request_body=PresignInput,responses((status=200,body=Presigned),(status=403),(status=422)))]
pub async fn presign(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<PresignInput>,
) -> Result<Json<Presigned>> {
    let id = admin(&state, &headers, true).await?;
    Ok(Json(
        uploads::presign(&state.db().await?, &state.config().await, id, input).await?,
    ))
}
#[utoipa::path(operation_id="media_register", post,path="/api/v1/admin/media",request_body=RegisterInput,responses((status=201,body=Media),(status=403),(status=422)))]
pub async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<RegisterInput>,
) -> Result<(StatusCode, Json<Media>)> {
    let id = admin(&state, &headers, true).await?;
    Ok((
        StatusCode::CREATED,
        Json(uploads::register(&state.db().await?, &state.config().await, id, input).await?),
    ))
}
#[utoipa::path(operation_id="media_get_media", get,path="/api/v1/admin/media/{id}",params(("id"=Uuid,Path)),responses((status=200,body=Media),(status=403),(status=404)))]
pub async fn get_media(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<Media>> {
    admin(&state, &headers, false).await?;
    Ok(Json(
        uploads::get(&state.db().await?, &state.config().await, id).await?,
    ))
}
#[utoipa::path(operation_id="media_public_media", get,path="/api/v1/media/{id}",params(("id"=Uuid,Path)),responses((status=200,body=Media),(status=404)))]
pub async fn public_media(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Media>> {
    let media = uploads::get(&state.db().await?, &state.config().await, id).await?;
    if !media.ready {
        return Err(AppError::NotFound);
    }
    Ok(Json(media))
}
#[utoipa::path(operation_id="media_delete", delete,path="/api/v1/admin/media/{id}",params(("id"=Uuid,Path)),responses((status=204),(status=403),(status=404)))]
pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<StatusCode> {
    admin(&state, &headers, true).await?;
    uploads::delete(&state.db().await?, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
