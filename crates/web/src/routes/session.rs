use crate::{AppError, error::Result, state::AppState};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

pub(super) const CSRF_COOKIE: &str = "sloray_csrf";

pub(super) fn csrf_token(headers: &HeaderMap) -> String {
    sloray_auth::read_cookie(headers, CSRF_COOKIE)
        .filter(|v| v.len() == 64 && v.bytes().all(|c| c.is_ascii_hexdigit()))
        .unwrap_or_else(sloray_auth::generate_token)
}

pub(super) fn csrf_cookie(token: &str, secure: bool) -> String {
    format!(
        "{CSRF_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax{}",
        if secure { "; Secure" } else { "" }
    )
}

pub(super) async fn optional_user(state: &AppState, headers: &HeaderMap) -> Result<Option<Uuid>> {
    match sloray_auth::read_cookie(headers, sloray_auth::SESSION_COOKIE) {
        Some(token) => Ok(sloray_auth::user_for_session(&state.db().await?, &token)
            .await?
            .map(|u| u.id)),
        None => Ok(None),
    }
}

#[derive(Serialize, ToSchema)]
#[schema(as = SessionUser)]
pub struct User {
    id: Uuid,
    display_name: String,
    email: String,
    role: String,
}

#[derive(Serialize, ToSchema)]
pub struct Session {
    user: Option<User>,
    csrf_token: String,
}

pub(super) fn verify_csrf(headers: &HeaderMap) -> Result<()> {
    let cookie = sloray_auth::read_cookie(headers, CSRF_COOKIE).ok_or(AppError::Forbidden)?;
    let token = headers
        .get("x-csrf-token")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Forbidden)?;
    if cookie.len() != 64 || !sloray_auth::constant_time_eq(cookie.as_bytes(), token.as_bytes()) {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

#[utoipa::path(operation_id="session_session", get, path="/api/v1/auth/session", responses((status=200,body=Session)))]
pub async fn session(State(state): State<AppState>, headers: HeaderMap) -> Result<Response> {
    let user = if let Some(token) = sloray_auth::read_cookie(&headers, sloray_auth::SESSION_COOKIE)
    {
        sloray_auth::user_for_session(&state.db().await?, &token)
            .await?
            .map(|user| User {
                id: user.id,
                display_name: user.display_name,
                email: user.email,
                role: user.role,
            })
    } else {
        None
    };
    let csrf_token = sloray_auth::read_cookie(&headers, CSRF_COOKIE)
        .filter(|v| v.len() == 64 && v.bytes().all(|c| c.is_ascii_hexdigit()))
        .unwrap_or_else(sloray_auth::generate_token);
    let secure = sloray_auth::cookie_is_secure(state.config().await.public_site_url.as_deref());
    let cookie = format!(
        "{CSRF_COOKIE}={csrf_token}; Path=/; HttpOnly; SameSite=Lax{}",
        if secure { "; Secure" } else { "" }
    );
    let mut response = Json(Session { user, csrf_token }).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        cookie
            .parse()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("cookie: {e}")))?,
    );
    Ok(response)
}
