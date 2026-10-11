//! 站点内容设置独立提交，不触碰部署期凭据。
use super::{html_response, require_admin};
use crate::{html, state::AppState};
use axum::{
    Form,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Response,
};
use sloray_content::{
    Actor,
    site::{self, SiteIdentity},
};

pub async fn form(state: &AppState) -> String {
    let Some(db) = state.db_optional().await else {
        return html::notice("warn", "数据库不可用，站点标识暂时无法编辑。");
    };
    match site::get(&db).await {
        Ok(value) => render(&value),
        Err(_) => html::notice("warn", "站点标识暂时不可用，请先应用最新迁移。"),
    }
}
fn render(value: &SiteIdentity) -> String {
    format!(
        r##"<form method="post" action="/admin/settings/site">
<fieldset><legend>站点标识</legend>
<input type="hidden" name="version" value="{}">
<label for="site_name">站点名称</label><input id="site_name" name="site_name" required maxlength="100" value="{}">
<label for="favicon_url">Favicon 图片地址</label><input id="favicon_url" name="favicon_url" maxlength="2048" value="{}" placeholder="/favicon.svg 或 https://…/icon.webp">
<p class="hint">可从媒体库复制公开图片地址。留空使用内置图标；名称同步到导航、页面标题和 RSS。</p>
<button type="submit" class="filled"><svg class="icon" aria-hidden="true"><use href="#mi-save"/></svg>保存站点标识</button>
</fieldset></form>"##,
        value.version,
        html::escape(&value.site_name),
        html::escape(value.favicon_url.as_deref().unwrap_or(""))
    )
}
pub async fn submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(input): Form<SiteIdentity>,
) -> Response {
    let user = match require_admin(&state, &headers).await {
        Ok(user) => user,
        Err(response) => return *response,
    };
    let db = match state.db().await {
        Ok(db) => db,
        Err(e) => return axum::response::IntoResponse::into_response(e),
    };
    let result = site::save(
        &db,
        Actor {
            id: user.id,
            admin: true,
        },
        input.clone(),
    )
    .await;
    let (status, message) = match result {
        Ok(_) => (
            StatusCode::OK,
            html::notice("ok", "站点标识已保存，公开页面刷新后生效。"),
        ),
        Err(sloray_content::Error::Conflict) => (
            StatusCode::CONFLICT,
            html::notice(
                "err",
                "设置已被其他窗口修改；当前输入保留，请复制后重新打开设置页。",
            ),
        ),
        Err(sloray_content::Error::Validation(message)) => {
            (StatusCode::BAD_REQUEST, html::notice("err", &message))
        }
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            html::notice("err", "保存失败，请稍后重试。"),
        ),
    };
    let value = if status == StatusCode::OK {
        site::get(&db).await.unwrap_or(input)
    } else {
        input
    };
    html_response(
        html::page(
            "站点标识",
            &format!(
                "<h1>站点标识</h1>{message}{}<p><a href=\"/admin/settings\">返回设置页</a></p>",
                render(&value)
            ),
        ),
        status,
    )
}
