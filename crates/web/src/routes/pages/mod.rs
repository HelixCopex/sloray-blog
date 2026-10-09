//! setup 向导 / 登录 / 设置页 —— 全部由 Rust 自己渲染 HTML。
//!
//! 为什么不用 Astro 渲染这几页：`/setup` 必须能在
//! "还没有数据库、可能还没有 Nginx、域名、TLS" 的时刻工作。
//! 此时唯一确定可用的进程就是 API 自己，所以它必须自包含、零外部依赖。
//!
//! 一切进入 HTML 的动态值都经过 [`crate::html::escape`]。
//!
//! 这个模块按页面拆成三个子模块；共享的鉴权辅助与响应构造留在本文件。
//! 每个文件都保持在 500 行以内（见 AGENTS.md 的硬性约束）。

use axum::{
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
};

use sloray_auth as auth;
use sloray_setup::SetupMode;

use crate::{html, state::AppState};

pub mod login;
pub mod settings;
pub mod setup;

pub use login::{login_form, login_submit, logout};
pub use settings::{settings_form, settings_submit};
pub use setup::{setup_form, setup_submit};

// ---------------------------------------------------------------- 小工具

/// 取当前登录用户。没有会话 / 会话过期 / token 无效都返回 None。
pub(super) async fn current_user(
    state: &AppState,
    headers: &HeaderMap,
) -> Option<sloray_store::entity::users::Model> {
    let token = auth::read_cookie(headers, auth::SESSION_COOKIE)?;
    let db = state.db_optional().await?;
    auth::user_for_session(&db, &token).await.ok().flatten()
}

/// 需要管理员身份。失败时返回一个已经构造好的响应。
///
/// Err 侧装箱：`Response` 有 128 字节以上，不装箱会让每个调用点的
/// `Result` 都白白变大（clippy 的 `result_large_err`）。
/// 这里的 Err 只走"未登录/无权限"路径，多一次分配可以忽略。
pub(super) async fn require_admin(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<sloray_store::entity::users::Model, Box<Response>> {
    match current_user(state, headers).await {
        Some(user) if user.is_admin() => Ok(user),
        // 已登录但不是管理员 —— 明确 403，而不是装作没登录。
        Some(_) => Err(Box::new(
            (
                StatusCode::FORBIDDEN,
                Html(html::page(
                    "权限不足",
                    "<h1>权限不足</h1><p>该页面仅限管理员。</p>",
                )),
            )
                .into_response(),
        )),
        None => Err(Box::new(Redirect::to("/login").into_response())),
    }
}

/// 当前 setup 状态（连同"数据库是否可达"一起算）。
pub(super) async fn setup_mode(state: &AppState) -> SetupMode {
    let config = state.config().await;
    sloray_setup::database_mode(&config, state.db_optional().await.as_ref()).await
}

pub(super) fn html_response(body: String, status: StatusCode) -> Response {
    (status, Html(body)).into_response()
}

/// 带 Set-Cookie 的重定向。
pub(super) fn redirect_with_cookie(location: &str, cookie: String) -> Response {
    let mut response = Redirect::to(location).into_response();
    match HeaderValue::from_str(&cookie) {
        Ok(value) => {
            response.headers_mut().insert(header::SET_COOKIE, value);
        }
        Err(err) => {
            // Cookie 全部由我们按固定格式拼接（十六进制 token + 固定属性），
            // 走到这里说明出了预期外的情况，宁可报错也不要静默丢弃会话。
            tracing::error!(error = %err, "Set-Cookie 构造失败");
            return (StatusCode::INTERNAL_SERVER_ERROR, "cookie error").into_response();
        }
    }
    response
}
