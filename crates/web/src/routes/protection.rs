//! 同源表单 CSRF 和跨实例限流在业务 handler 前执行。
use crate::{AppError, state::AppState};
use axum::{
    body::{Body, to_bytes},
    extract::{ConnectInfo, Request, State},
    http::{HeaderValue, Method, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::net::{IpAddr, SocketAddr};

#[derive(Clone)]
pub struct ClientHash(pub Vec<u8>);

pub async fn protect(State(state): State<AppState>, mut request: Request, next: Next) -> Response {
    let path = request.uri().path().to_owned();
    let writing = !matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    );
    if writing && ["/login", "/logout", "/admin/settings"].contains(&path.as_str()) {
        let cookie = sloray_auth::read_cookie(request.headers(), super::session::CSRF_COOKIE);
        let (parts, body) = request.into_parts();
        let bytes = match to_bytes(body, 2_000_000).await {
            Ok(bytes) => bytes,
            Err(_) => return AppError::BadRequest("表单过大或无法读取".into()).into_response(),
        };
        let token = url::form_urlencoded::parse(&bytes)
            .find(|(k, _)| k == "csrf_token")
            .map(|(_, v)| v.into_owned());
        if cookie
            .as_ref()
            .zip(token.as_ref())
            .is_none_or(|(cookie, token)| {
                cookie.len() != 64
                    || !sloray_auth::constant_time_eq(cookie.as_bytes(), token.as_bytes())
            })
        {
            return AppError::Forbidden.into_response();
        }
        request = Request::from_parts(parts, Body::from(bytes));
    }
    let rule = if writing && path == "/api/v1/admin/users" {
        Some(("users", 60, 30))
    } else if writing && path.starts_with("/api/v1/admin/media") {
        Some(("media", 60, 30))
    } else if writing && path == "/login" {
        Some(("login", 900, 10))
    } else if writing && path.ends_with("/comments") && !path.contains("/admin/") {
        Some(("comments", 600, 5))
    } else if writing && path.ends_with("/view") {
        Some(("views", 60, 120))
    } else if writing && path.ends_with("/like") {
        Some(("likes", 60, 60))
    } else {
        None
    };
    if let Some((scope, window, maximum)) = rule {
        let db = match state.db().await {
            Ok(db) => db,
            Err(e) => return e.into_response(),
        };
        let peer = request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|p| p.0.ip());
        // Nginx 上游只能经 loopback 到达；公网直连时忽略伪造的代理头。
        let address = peer
            .filter(|p| p.is_loopback())
            .and_then(|_| request.headers().get("x-real-ip"))
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<IpAddr>().ok())
            .or(peer);
        let subject = address
            .map(|v| v.to_string())
            .unwrap_or_else(|| "unknown".into());
        let hash = match sloray_auth::limits::private_hash(&db, &subject).await {
            Ok(hash) => hash,
            Err(e) => return AppError::Database(e).into_response(),
        };
        match sloray_auth::limits::allow(&db, scope, &hash, window, maximum).await {
            Ok(true) => {
                request.extensions_mut().insert(ClientHash(hash));
            }
            Ok(false) => return AppError::RateLimited.into_response(),
            Err(e) => return AppError::Database(e).into_response(),
        }
    }
    let csrf = super::session::csrf_token(request.headers());
    let secure = sloray_auth::cookie_is_secure(state.config().await.public_site_url.as_deref());
    let mut response = next.run(request).await;
    // 自渲染管理页所有 form 都插入相同 token，错误回显页也保留可重试能力。
    if ["/login", "/admin/settings"].contains(&path.as_str())
        && response
            .headers()
            .get(header::CONTENT_TYPE)
            .is_some_and(|v| v.as_bytes().starts_with(b"text/html"))
    {
        let (mut parts, body) = response.into_parts();
        match to_bytes(body, 2_000_000).await {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                let mut html = String::with_capacity(text.len() + 200);
                let mut rest = text.as_ref();
                while let Some(start) = rest.find("<form ") {
                    let Some(end) = rest[start..].find('>') else {
                        break;
                    };
                    let end = start + end + 1;
                    html.push_str(&rest[..end]);
                    html.push_str(&format!(
                        "<input type=\"hidden\" name=\"csrf_token\" value=\"{csrf}\">"
                    ));
                    rest = &rest[end..];
                }
                html.push_str(rest);
                parts.headers.remove(header::CONTENT_LENGTH);
                parts.headers.append(
                    header::SET_COOKIE,
                    HeaderValue::from_str(&super::session::csrf_cookie(&csrf, secure))
                        .expect("hex token cookie"),
                );
                response = Response::from_parts(parts, Body::from(html));
            }
            Err(e) => return AppError::Internal(anyhow::anyhow!(e)).into_response(),
        }
    }
    response
}
