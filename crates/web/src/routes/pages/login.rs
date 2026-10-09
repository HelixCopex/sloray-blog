//! `/login` `/logout`：会话建立与销毁。

use axum::{
    Form,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;

use sloray_auth as auth;

use super::{html_response, redirect_with_cookie};
use crate::{html, state::AppState};

// ---------------------------------------------------------------- 登录

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    #[serde(default)]
    email: String,
    #[serde(default)]
    password: String,
}

fn render_login(notice: Option<(&str, &str)>, email: &str) -> String {
    let notice_html = notice.map(|(k, m)| html::notice(k, m)).unwrap_or_default();
    let body = format!(
        r#"{notice_html}
<h1><svg class="icon" aria-hidden="true"><use href='#mi-person'/></svg> 登录</h1>
<form method="post" action="/login">
  <label for="email">邮箱</label>
  <input id="email" name="email" type="email" required value="{email}">
  <label for="password">密码</label>
  <input id="password" name="password" type="password" required autocomplete="current-password">
  <button type="submit"><svg class="icon" aria-hidden="true"><use href='#mi-person'/></svg> 登录</button>
</form>"#,
        notice_html = notice_html,
        email = html::escape(email),
    );
    html::page("登录", &body)
}

/// `GET /login`
pub async fn login_form(State(state): State<AppState>, headers: HeaderMap) -> Response {
    // 还没配置就没法登录（认证要查数据库），直接送去 setup。
    if !state.is_configured().await {
        return Redirect::to("/setup").into_response();
    }
    if super::current_user(&state, &headers).await.is_some() {
        return Redirect::to("/admin/posts").into_response();
    }
    html_response(render_login(None, ""), StatusCode::OK)
}

/// `POST /login`
pub async fn login_submit(State(state): State<AppState>, Form(form): Form<LoginForm>) -> Response {
    let config = state.config().await;
    let secure = auth::cookie_is_secure(config.public_site_url.as_deref());

    let email = auth::normalize_email(&form.email);
    let fail = || {
        // 刻意不区分"用户不存在"和"密码错误"：那会变成一个账号枚举接口。
        html_response(
            render_login(Some(("err", "邮箱或密码不正确。")), &email),
            StatusCode::UNAUTHORIZED,
        )
    };

    let Ok(db) = state.db().await else {
        return Redirect::to("/setup").into_response();
    };

    if form.password.len() > 1024 || email.len() > 254 {
        return fail();
    }
    let Ok(Some(user)) = auth::find_user_by_email(&db, &email).await else {
        // 不存在的账户也执行同参数 Argon2，避免明显的耗时枚举。
        let _ = tokio::task::spawn_blocking(move || auth::hash_password(&form.password)).await;
        return fail();
    };

    let hash = user.password_hash.clone();
    if !tokio::task::spawn_blocking(move || auth::verify_password(&form.password, &hash))
        .await
        .unwrap_or(false)
    {
        return fail();
    }

    match auth::create_session(&db, user.id, None).await {
        Ok(token) => redirect_with_cookie("/admin/posts", auth::session_cookie(&token, secure)),
        Err(err) => {
            tracing::error!(error = %err, "创建会话失败");
            html_response(
                render_login(Some(("err", "服务暂时不可用，请稍后再试。")), &email),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        }
    }
}

/// `POST /logout`
pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let config = state.config().await;
    let secure = auth::cookie_is_secure(config.public_site_url.as_deref());

    if let Some(token) = auth::read_cookie(&headers, auth::SESSION_COOKIE)
        && let Some(db) = state.db_optional().await
        && let Err(err) = auth::destroy_session(&db, &token).await
    {
        // 登出失败不该阻塞用户；顺手记一条日志。
        tracing::warn!(error = %err, "销毁会话失败");
    }

    redirect_with_cookie("/login", auth::clear_session_cookie(secure))
}
