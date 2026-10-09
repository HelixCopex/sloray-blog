use crate::{AppError, error::Result, state::AppState};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use sloray_auth::accounts::{CreateUser, User};
async fn admin(state: &AppState, headers: &HeaderMap, writing: bool) -> Result<()> {
    if !super::content::actor(state, headers, writing).await?.admin {
        return Err(AppError::Forbidden);
    }
    Ok(())
}
#[utoipa::path(operation_id="users_list", get,path="/api/v1/admin/users",responses((status=200,body=Vec<User>),(status=401),(status=403)))]
pub async fn list(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<Vec<User>>> {
    admin(&state, &headers, false).await?;
    Ok(Json(sloray_auth::accounts::list(&state.db().await?).await?))
}
#[utoipa::path(operation_id="users_create", post,path="/api/v1/admin/users",request_body=CreateUser,responses((status=201,body=User),(status=401),(status=403),(status=409),(status=422)))]
pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<CreateUser>,
) -> Result<(StatusCode, Json<User>)> {
    admin(&state, &headers, true).await?;
    sloray_auth::accounts::validate(&input).map_err(AppError::Validation)?;
    let user = sloray_auth::create_user(
        &state.db().await?,
        &input.email,
        &input.password,
        input.display_name.trim(),
        &input.role,
    )
    .await
    .map_err(|e| {
        if e.downcast_ref::<sea_orm::DbErr>().is_some_and(|e| {
            matches!(
                e.sql_err(),
                Some(sea_orm::SqlErr::UniqueConstraintViolation(_))
            )
        }) {
            AppError::Conflict
        } else {
            AppError::Internal(e)
        }
    })?;
    Ok((StatusCode::CREATED, Json(user.into())))
}
