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
<header class="page-heading">
<h1><svg class="icon" aria-hidden="true"><use href='#mi-settings'/></svg> 设置</h1>
<p>管理站点标识、公开地址与媒体服务。</p>
<p class="account-label">已登录：{who}（{role}）</p>
</header>
<details class="settings-dock" open><summary aria-label="展开或收起后台导航" title="展开或收起后台导航"><svg class="icon" aria-hidden="true"><use href='#mi-menu'/></svg></summary><nav class="settings-nav" aria-label="后台导航">
<a href="/admin/posts" aria-label="文章" title="文章"><svg class="icon" aria-hidden="true"><use href='#mi-edit'/></svg></a><a href="/admin/pages" aria-label="页面" title="页面"><svg class="icon" aria-hidden="true"><use href='#mi-article'/></svg></a><a href="/admin/media" aria-label="媒体" title="媒体"><svg class="icon" aria-hidden="true"><use href='#mi-image'/></svg></a><a href="/admin/comments" aria-label="评论" title="评论"><svg class="icon" aria-hidden="true"><use href='#mi-comment'/></svg></a><a href="/admin/themes" aria-label="主题" title="主题"><svg class="icon" aria-hidden="true"><use href='#mi-palette'/></svg></a><a href="/admin/users" aria-label="账号" title="账号"><svg class="icon" aria-hidden="true"><use href='#mi-person'/></svg></a><a href="/admin/settings" aria-current="page" aria-label="设置" title="设置"><svg class="icon" aria-hidden="true"><use href='#mi-settings'/></svg></a></nav></details>
{identity_form}
<aside class="settings-help"><svg class="icon" aria-hidden="true"><use href='#mi-palette'/></svg><p>背景、颜色与页面布局在<a href="/admin/themes">主题工作台</a>设置，保存后启用。</p></aside>

<form class="settings-form" method="post" action="/admin/settings">
  <section class="form-card" aria-labelledby="site-address-heading">
    <h2 id="site-address-heading">站点地址</h2>
    <p class="hint" id="site-address-help">用于文章、RSS 与站点地图的公开地址。</p>
    <label for="site_url">对外地址</label>
    <input id="site_url" name="site_url" type="text"
           placeholder="https://blog.example.com" value="{site_url}" aria-describedby="site-address-help">
  </section>

  <section class="form-card" aria-labelledby="storage-heading">
    <h2 id="storage-heading">对象存储</h2>
    <p class="hint" id="storage-help">保存私有原图；原图 Bucket 需关闭公开访问。</p>
    <div class="form-grid">
    <div class="field-full">
    <label for="s3_endpoint">Endpoint</label>
    <input id="s3_endpoint" name="s3_endpoint" type="text" value="{s3_endpoint}" aria-describedby="storage-help">
    </div><div>
    <label for="s3_region">Region</label>
    <input id="s3_region" name="s3_region" type="text" value="{s3_region}">
    </div><div>
    <label for="s3_bucket">私有原图 Bucket</label>
    <input id="s3_bucket" name="s3_bucket" type="text" value="{s3_bucket}" aria-describedby="storage-help">
    </div><div>
    <label for="s3_access_key_id">Access Key ID</label>
    <input id="s3_access_key_id" name="s3_access_key_id" type="text" value="{s3_access_key_id}">
    </div><div>
    <label for="s3_secret_access_key">Secret Access Key</label>
    <input id="s3_secret_access_key" name="s3_secret_access_key" type="password"
           autocomplete="off" placeholder="留空保持当前值" aria-describedby="storage-secret-help">
    <p class="hint" id="storage-secret-help">已保存的密钥不回显，留空保持当前值。</p>
    </div></div>
  </section>

  <section class="form-card" aria-labelledby="media-settings-heading">
    <h2 id="media-settings-heading">图片与缓存</h2>
    <p class="hint" id="media-settings-help">公开图片与私有原图使用不同的 Bucket。</p>
    <div class="form-grid">
    <div>
    <label for="media_bucket">公开派生图 Bucket</label>
    <input id="media_bucket" name="media_public_bucket" value="{media_bucket}" aria-describedby="media-settings-help">
    </div><div>
    <label for="media_url">媒体公开地址</label>
    <input id="media_url" name="media_public_base_url" type="url" value="{media_url}">
    </div><div>
    <label for="cdn_zone">Cloudflare Zone ID</label>
    <input id="cdn_zone" name="cloudflare_zone_id" value="{cdn_zone}">
    </div><div>
    <label for="cdn_token">CDN 清理 API Token</label>
    <input id="cdn_token" name="cloudflare_api_token" type="password" autocomplete="off" aria-describedby="cdn-token-help">
    <p class="hint" id="cdn-token-help">留空保持已保存的 Token。</p>
    </div></div>
    <label class="checkbox-field"><input type="checkbox" name="clear_cdn" value="true">移除 CDN 清理配置</label>
  </section>

  <section class="form-card" aria-labelledby="database-heading">
    <h2 id="database-heading">数据库</h2>
    <p class="hint" id="database-current">当前：<code>{masked}</code></p>
    <label for="db_url">新的连接串</label>
    <input id="db_url" name="db_url" type="text" autocomplete="off"
           placeholder="留空保持当前连接" aria-describedby="database-current database-help">
    <p class="hint" id="database-help">留空保持当前连接。修改时需完整填写连接串；提交后先验证连接和迁移，成功才切换。</p>
  </section>
  <div class="form-actions"><button type="submit" class="filled"><svg class="icon" aria-hidden="true"><use href='#mi-save'/></svg> 保存</button><p class="hint">保存地址、存储与缓存配置。</p></div>
</form>

<div class="settings-footer"><p>站点标识使用上方的独立保存按钮。</p><form method="post" action="/logout">
  <button class="text" type="submit"><svg class="icon" aria-hidden="true"><use href='#mi-logout'/></svg> 退出登录</button>
</form></div>"#,
        notice_html = notice_html,
        who = html::escape(&user.display_name),
        role = if user.role == "admin" {
            "管理员"
        } else {
            "作者"
        },
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
