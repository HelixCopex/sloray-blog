//! `/admin/settings`：登录后修改数据库 / 对象存储 / 站点地址。

use axum::{
    Form,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Response,
};
use serde::Deserialize;

use sloray_setup as setup;

use super::{html_response, require_admin};
use crate::{html, state::AppState};

// ---------------------------------------------------------------- 设置

#[derive(Debug, Deserialize)]
pub struct SettingsForm {
    #[serde(default)]
    media_public_base_url: String,
    #[serde(default)]
    media_public_bucket: String,
    #[serde(default)]
    cloudflare_zone_id: String,
    #[serde(default)]
    cloudflare_api_token: String,
    #[serde(default)]
    clear_cdn: bool,
    #[serde(default)]
    site_url: String,
    /// 留空表示"不改数据库连接串"。
    #[serde(default)]
    db_url: String,
    #[serde(default)]
    s3_endpoint: String,
    #[serde(default)]
    s3_region: String,
    #[serde(default)]
    s3_bucket: String,
    #[serde(default)]
    s3_access_key_id: String,
    #[serde(default)]
    s3_secret_access_key: String,
}

fn render_settings(
    user: &sloray_store::entity::users::Model,
    config: &sloray_config::Config,
    notice: Option<(&str, &str)>,
    identity_form: &str,
) -> String {
    let notice_html = notice.map(|(k, m)| html::notice(k, m)).unwrap_or_default();
    let masked = config
        .database_url
        .as_deref()
        .map(sloray_config::mask_database_url)
        .unwrap_or_else(|| "（未配置）".to_owned());

    let opt = |v: &Option<String>| html::escape(v.as_deref().unwrap_or(""));

    let body = format!(
        r#"{notice_html}
<h1><svg class="icon" aria-hidden="true"><use href='#mi-settings'/></svg> 设置</h1>
<p class="sub">已登录：{who}（{role}）</p>
<details class="settings-dock" open><summary><svg class="icon" aria-hidden="true"><use href='#mi-menu'/></svg> 导航</summary><nav class="settings-nav" aria-label="后台导航">
<a href="/admin/posts"><svg class="icon" aria-hidden="true"><use href='#mi-edit'/></svg>文章</a><a href="/admin/pages"><svg class="icon" aria-hidden="true"><use href='#mi-article'/></svg>页面</a><a href="/admin/media"><svg class="icon" aria-hidden="true"><use href='#mi-image'/></svg>媒体</a><a href="/admin/comments"><svg class="icon" aria-hidden="true"><use href='#mi-comment'/></svg>评论</a><a href="/admin/themes"><svg class="icon" aria-hidden="true"><use href='#mi-palette'/></svg>主题</a><a href="/admin/users"><svg class="icon" aria-hidden="true"><use href='#mi-person'/></svg>账号</a><a href="/admin/settings" aria-current="page"><svg class="icon" aria-hidden="true"><use href='#mi-settings'/></svg>设置</a></nav></details>
{identity_form}

<form method="post" action="/admin/settings">
  <fieldset>
    <legend>站点地址</legend>
    <label for="site_url">对外地址</label>
    <input id="site_url" name="site_url" type="text"
           placeholder="https://blog.example.com" value="{site_url}">
    <p class="hint">保存后用于文章、RSS 与站点地图的公开地址。</p>
  </fieldset>

  <fieldset>
    <legend>数据库</legend>
    <p class="hint">当前：<code>{masked}</code></p>
    <label for="db_url">改成新的连接串（留空则不变）</label>
    <input id="db_url" name="db_url" type="text" autocomplete="off"
           placeholder="留空保持当前连接">
    <p class="hint">
      提交时会先测试新连接、必要时跑迁移，成功后才切换。
      注意：**密码不会回显**，所以想改其他部分也必须完整重填连接串。
    </p>
  </fieldset>

  <fieldset>
    <legend>对象存储</legend>
    <label for="s3_endpoint">Endpoint</label>
    <input id="s3_endpoint" name="s3_endpoint" type="text" value="{s3_endpoint}">
    <label for="s3_region">Region</label>
    <input id="s3_region" name="s3_region" type="text" value="{s3_region}">
    <label for="s3_bucket">私有原图 Bucket（关闭公开访问）</label>
    <input id="s3_bucket" name="s3_bucket" type="text" value="{s3_bucket}">
    <label for="s3_access_key_id">Access Key ID</label>
    <input id="s3_access_key_id" name="s3_access_key_id" type="text" value="{s3_access_key_id}">
    <label for="s3_secret_access_key">Secret Access Key（留空表示不变）</label>
    <input id="s3_secret_access_key" name="s3_secret_access_key" type="password"
           autocomplete="off" placeholder="留空保持当前值">
  </fieldset>

  <fieldset><legend>图片与缓存</legend>
    <label for="media_bucket">公开派生图 Bucket（必须与原图分开）</label><input id="media_bucket" name="media_public_bucket" value="{media_bucket}">
    <label for="media_url">媒体公开地址</label><input id="media_url" name="media_public_base_url" type="url" value="{media_url}">
    <label for="cdn_zone">Cloudflare Zone ID</label><input id="cdn_zone" name="cloudflare_zone_id" value="{cdn_zone}">
    <label for="cdn_token">CDN 清理 API Token（留空保持）</label><input id="cdn_token" name="cloudflare_api_token" type="password" autocomplete="off">
    <label><input type="checkbox" name="clear_cdn" value="true">移除 CDN 清理配置</label>
  </fieldset>
  <button type="submit"><svg class="icon" aria-hidden="true"><use href='#mi-save'/></svg> 保存</button>
</form>

<form method="post" action="/logout" style="margin-top:1.5rem">
  <button class="secondary" type="submit"><svg class="icon" aria-hidden="true"><use href='#mi-logout'/></svg> 退出登录</button>
</form>"#,
        notice_html = notice_html,
        who = html::escape(&user.display_name),
        role = html::escape(&user.role),
        site_url = opt(&config.public_site_url),
        media_url = opt(&config.media_public_base_url),
        media_bucket = opt(&config.media_public_bucket),
        cdn_zone = opt(&config.cloudflare_zone_id),
        masked = html::escape(&masked),
        s3_endpoint = opt(&config.s3.endpoint),
        s3_region = opt(&config.s3.region),
        s3_bucket = opt(&config.s3.bucket),
        s3_access_key_id = opt(&config.s3.access_key_id),
    );

    html::page("设置", &body)
}

/// `GET /admin/settings`
pub async fn settings_form(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let user = match require_admin(&state, &headers).await {
        Ok(user) => user,
        Err(resp) => return *resp,
    };
    let config = state.config().await;
    let identity_form = super::identity::form(&state).await;
    let notice = if !config.is_configured() {
        Some(("warn", "实例还未完成配置，请先访问 /setup。"))
    } else {
        None
    };
    html_response(
        render_settings(&user, &config, notice, &identity_form),
        StatusCode::OK,
    )
}

/// `POST /admin/settings`
///
/// 数据库连接串的变更走"先验证、后切换"：新连接连不上就**不动现有配置**，
/// 避免把一台本来能跑的实例改成完全起不来。
pub async fn settings_submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<SettingsForm>,
) -> Response {
    let user = match require_admin(&state, &headers).await {
        Ok(user) => user,
        Err(resp) => return *resp,
    };

    let mut config = state.config().await;
    let identity_form = super::identity::form(&state).await;

    let mut replacement_db = state.db_optional().await;

    // ---- 可选的数据库变更 ----
    let new_db = form.db_url.trim();
    if !new_db.is_empty() {
        if !new_db.starts_with("postgres://") && !new_db.starts_with("postgresql://") {
            return html_response(
                render_settings(
                    &user,
                    &config,
                    Some(("err", "数据库连接串格式不正确。")),
                    &identity_form,
                ),
                StatusCode::BAD_REQUEST,
            );
        }
        let db = match setup::probe_database(new_db, &config).await {
            Ok(db) => db,
            Err(msg) => {
                // 关键：此时还**没有**改动任何已生效的配置。
                return html_response(
                    render_settings(&user, &config, Some(("err", &msg)), &identity_form),
                    StatusCode::BAD_REQUEST,
                );
            }
        };
        if let Err(err) = setup::run_migrations(&db).await {
            tracing::error!(error = %err, "settings：新数据库迁移失败");
            return html_response(
                render_settings(
                    &user,
                    &config,
                    Some(("err", "连接成功但迁移失败，配置未改动。")),
                    &identity_form,
                ),
                StatusCode::BAD_REQUEST,
            );
        }

        config.database_url = Some(new_db.to_owned());
        replacement_db = Some(db);
    }

    // ---- 站点地址 ----
    let site = form.site_url.trim();
    config.public_site_url = (!site.is_empty()).then(|| site.to_owned());

    // ---- 对象存储 ----
    let secret_provided = !form.s3_secret_access_key.trim().is_empty();
    let mut s3 = sloray_config::S3Config {
        endpoint: non_empty(&form.s3_endpoint),
        region: non_empty(&form.s3_region),
        bucket: non_empty(&form.s3_bucket),
        access_key_id: non_empty(&form.s3_access_key_id),
        // 密码框留空表示"保持不变"，而不是"清空" —— 否则每次保存都会把
        // 无法回显的 secret 抹掉。
        secret_access_key: if secret_provided {
            non_empty(&form.s3_secret_access_key)
        } else {
            config.s3.secret_access_key.clone()
        },
    };
    if s3.endpoint.is_none()
        && s3.region.is_none()
        && s3.bucket.is_none()
        && s3.access_key_id.is_none()
        && !secret_provided
    {
        // 全部留空 → 认为用户想清掉对象存储配置。
        s3 = sloray_config::S3Config::default();
    }
    config.s3 = s3;
    config.media_public_base_url = non_empty(&form.media_public_base_url);
    config.media_public_bucket = non_empty(&form.media_public_bucket);
    if form.clear_cdn {
        config.cloudflare_zone_id = None;
        config.cloudflare_api_token = None;
    } else {
        config.cloudflare_zone_id = non_empty(&form.cloudflare_zone_id);
        if !form.cloudflare_api_token.trim().is_empty() {
            config.cloudflare_api_token = non_empty(&form.cloudflare_api_token);
        }
    }

    if let Err(err) = config.validate() {
        return html_response(
            render_settings(
                &user,
                &config,
                Some(("err", &format!("配置校验失败：{err}"))),
                &identity_form,
            ),
            StatusCode::BAD_REQUEST,
        );
    }

    if let Err(err) = state.reconfigure(config.clone(), replacement_db).await {
        tracing::error!(error = %err, "settings：写入配置失败");
        return html_response(
            render_settings(
                &user,
                &config,
                Some(("err", "配置无法写入磁盘，请检查目录权限。")),
                &identity_form,
            ),
            StatusCode::INTERNAL_SERVER_ERROR,
        );
    }

    tracing::info!(actor = %user.email, "设置已更新");
    let identity_form = super::identity::form(&state).await;
    html_response(
        render_settings(&user, &config, Some(("ok", "已保存。")), &identity_form),
        StatusCode::OK,
    )
}

fn non_empty(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_owned())
}
