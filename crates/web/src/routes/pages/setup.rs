//! `/setup` 安装向导：未配置状态下的唯一入口。

use axum::{
    Form,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;

use sloray_auth as auth;
use sloray_setup::{self as setup, SetupMode};

use super::{html_response, redirect_with_cookie, setup_mode};
use crate::{html, state::AppState};

// ---------------------------------------------------------------- setup

#[derive(Debug, Deserialize)]
pub struct SetupForm {
    #[serde(default)]
    token: String,
    #[serde(default)]
    site_url: String,
    #[serde(default)]
    db_url: String,
    #[serde(default)]
    admin_email: String,
    #[serde(default)]
    admin_name: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    password2: String,
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

impl SetupForm {
    fn s3(&self) -> sloray_config::S3Config {
        fn opt(s: &str) -> Option<String> {
            let s = s.trim();
            (!s.is_empty()).then(|| s.to_owned())
        }
        sloray_config::S3Config {
            endpoint: opt(&self.s3_endpoint),
            region: opt(&self.s3_region),
            bucket: opt(&self.s3_bucket),
            access_key_id: opt(&self.s3_access_key_id),
            secret_access_key: opt(&self.s3_secret_access_key),
        }
    }
}

/// 渲染 setup 表单。`notice` 用于回显错误或提示。
fn render_setup(mode: SetupMode, notice: Option<(&str, &str)>, form: Option<&SetupForm>) -> String {
    let token_value = form.map(|f| f.token.as_str()).unwrap_or("");
    let site_url = form.map(|f| f.site_url.as_str()).unwrap_or("");
    let db_url = form.map(|f| f.db_url.as_str()).unwrap_or("");
    let admin_email = form.map(|f| f.admin_email.as_str()).unwrap_or("");
    let admin_name = form.map(|f| f.admin_name.as_str()).unwrap_or("");
    let s3 = |f: Option<&SetupForm>, pick: fn(&SetupForm) -> &str| {
        f.map(|f| pick(f).to_owned()).unwrap_or_default()
    };

    let banner = match mode {
        SetupMode::Required => html::notice(
            "warn",
            "这个实例还没有完成初始配置。请填写下面的内容 —— 提交后配置会写入磁盘并立即生效。",
        ),
        SetupMode::Recoverable => html::notice(
            "err",
            "已配置的数据库当前连不上。你可以在这里改正连接串（救急通道）。",
        ),
        SetupMode::Disabled => String::new(),
    };

    let notice_html = notice.map(|(k, m)| html::notice(k, m)).unwrap_or_default();

    let body = format!(
        r#"{banner}{notice_html}
<h1><svg class="icon" aria-hidden="true"><use href='#mi-settings'/></svg> 初始化 sloray-blog</h1>
<p class="sub">所有字段都可以之后再改（登录后进入「设置」）。</p>

<form method="post" action="/setup">
  <fieldset>
    <legend>部署凭据</legend>
    <label for="token">Setup token</label>
    <input id="token" name="token" type="password" autocomplete="off" required value="{token}">
    <p class="hint">
      从服务器上取：<code>sudo cat /srv/sloray/shared/setup-token</code>，
      或 <code>sudo journalctl -u sloray@8080 | grep setup</code>。
    </p>
  </fieldset>

  <fieldset>
    <legend>数据库</legend>
    <label for="db_url">Postgres 连接串</label>
    <input id="db_url" name="db_url" type="text" required
           placeholder="postgres://sloray:密码@127.0.0.1:5432/sloray" value="{db_url}">
    <p class="hint">
      数据库服务必须已启动，角色与数据库需要事先建好。
      连接串需填写实际用户名、密码、地址与端口。
      提交时会先测试连接，再自动执行迁移建表。
    </p>
  </fieldset>

  <fieldset>
    <legend>管理员账户</legend>
    <label for="admin_email">邮箱</label>
    <input id="admin_email" name="admin_email" type="email" required value="{admin_email}">
    <label for="admin_name">显示名称</label>
    <input id="admin_name" name="admin_name" type="text" value="{admin_name}" placeholder="站长">
    <label for="password">密码</label>
    <input id="password" name="password" type="password" required autocomplete="new-password">
    <label for="password2">再输一次</label>
    <input id="password2" name="password2" type="password" required autocomplete="new-password">
    <p class="hint">至少 8 位，且不能是 admin / password 这类常见口令。</p>
  </fieldset>

  <fieldset>
    <legend>站点地址</legend>
    <label for="site_url">对外地址</label>
    <input id="site_url" name="site_url" type="text"
           placeholder="https://blog.example.com" value="{site_url}">
    <p class="hint">
      用于 canonical 与媒体链接。注意：Astro 前端的 canonical 用的是**构建期**注入的值，
      这里改它不会改变已构建的前端产物。
    </p>
  </fieldset>

  <fieldset>
    <legend>对象存储（可跳过）</legend>
    <label for="s3_endpoint">Endpoint</label>
    <input id="s3_endpoint" name="s3_endpoint" type="text"
           placeholder="https://<account>.r2.cloudflarestorage.com" value="{s3_endpoint}">
    <label for="s3_region">Region</label>
    <input id="s3_region" name="s3_region" type="text" placeholder="auto" value="{s3_region}">
    <label for="s3_bucket">Bucket</label>
    <input id="s3_bucket" name="s3_bucket" type="text" value="{s3_bucket}">
    <label for="s3_access_key_id">Access Key ID</label>
    <input id="s3_access_key_id" name="s3_access_key_id" type="text" value="{s3_access_key_id}">
    <label for="s3_secret_access_key">Secret Access Key</label>
    <input id="s3_secret_access_key" name="s3_secret_access_key" type="password"
           autocomplete="off" value="{s3_secret_access_key}">
    <p class="hint">
      五个字段要么都填、要么都留空（媒体功能上线前留空即可）。配置见 docs/media.md。
    </p>
  </fieldset>

  <button type="submit"><svg class="icon" aria-hidden="true"><use href='#mi-check'/></svg> 完成初始化</button>
</form>"#,
        banner = banner,
        notice_html = notice_html,
        token = html::escape(token_value),
        db_url = html::escape(db_url),
        admin_email = html::escape(admin_email),
        admin_name = html::escape(admin_name),
        site_url = html::escape(site_url),
        s3_endpoint = html::escape(&s3(form, |f| &f.s3_endpoint)),
        s3_region = html::escape(&s3(form, |f| &f.s3_region)),
        s3_bucket = html::escape(&s3(form, |f| &f.s3_bucket)),
        s3_access_key_id = html::escape(&s3(form, |f| &f.s3_access_key_id)),
        s3_secret_access_key = html::escape(&s3(form, |f| &f.s3_secret_access_key)),
    );

    html::page("初始化 sloray-blog", &body)
}

/// `GET /setup`
pub async fn setup_form(State(state): State<AppState>) -> Response {
    match setup_mode(&state).await {
        SetupMode::Disabled => Redirect::to("/login").into_response(),
        mode => html_response(render_setup(mode, None, None), StatusCode::OK),
    }
}

/// `POST /setup`
///
/// 顺序刻意是：验证 token → 验证输入 → 试连数据库 → 跑迁移 → 建管理员 →
/// 保存配置 → 自动登录。**任何一步失败都不写盘**，
/// 所以不会留下"配置写了一半"的状态。
pub async fn setup_submit(State(state): State<AppState>, Form(form): Form<SetupForm>) -> Response {
    let config = state.config().await;
    let mode = setup_mode(&state).await;

    if mode == SetupMode::Disabled {
        return Redirect::to("/login").into_response();
    }

    let fail = |msg: &str| {
        html_response(
            render_setup(mode, Some(("err", msg)), Some(&form)),
            StatusCode::BAD_REQUEST,
        )
    };

    // ---- 1) token ----
    let token_path = setup::token_path(&config.config_file);
    let Some(expected) = setup::read_token(&token_path) else {
        tracing::error!(path = %token_path.display(), "setup token 文件不存在");
        return fail("服务器上找不到 setup token 文件，请检查服务日志。");
    };
    if !setup::verify_token(&form.token, &expected) {
        tracing::warn!("setup：token 校验失败");
        return fail("Setup token 不正确。");
    }

    // ---- 2) 输入 ----
    let db_url = form.db_url.trim();
    if !db_url.starts_with("postgres://") && !db_url.starts_with("postgresql://") {
        return fail("数据库连接串必须以 postgres:// 或 postgresql:// 开头。");
    }
    let email = auth::normalize_email(&form.admin_email);
    if !email.contains('@') {
        return fail("管理员邮箱格式不正确。");
    }
    if form.password != form.password2 {
        return fail("两次输入的密码不一致。");
    }
    if let Err(msg) = auth::validate_password(&form.password) {
        return fail(&msg);
    }
    let s3 = form.s3();
    if !s3.is_empty() && !s3.is_complete() {
        return fail("对象存储要么五个字段都填，要么都留空。");
    }

    // ---- 3) 试连数据库 ----
    let db = match setup::probe_database(db_url, &config).await {
        Ok(db) => db,
        Err(msg) => return fail(&msg),
    };

    // ---- 4) 迁移 ----
    if let Err(err) = setup::run_migrations(&db).await {
        tracing::error!(error = %err, "setup：迁移失败");
        return fail("数据库连接成功，但建表（迁移）失败，请查看服务日志。");
    }

    // ---- 5) 管理员 ----
    // 只在"库里还没有管理员"时创建；救急通道进来时通常已经有了，
    // 这时不该覆盖别人的账号。
    let created_admin = match auth::create_first_admin(
        &db,
        &email,
        &form.password,
        if form.admin_name.trim().is_empty() {
            "站长"
        } else {
            form.admin_name.trim()
        },
    )
    .await
    {
        Ok(user) => user,
        Err(err) => {
            tracing::error!(error = %err, "setup：创建管理员失败");
            return fail("创建管理员账户失败，请查看服务日志。");
        }
    };

    // ---- 6) 保存配置 ----
    let mut updated = config.clone();
    updated.database_url = Some(db_url.to_owned());
    updated.public_site_url = {
        let s = form.site_url.trim();
        (!s.is_empty()).then(|| s.to_owned())
    };
    updated.s3 = s3;

    if let Err(err) = updated.validate() {
        return fail(&format!("配置校验失败：{err}"));
    }
    if let Err(err) = state.reconfigure(updated.clone(), Some(db.clone())).await {
        tracing::error!(error = %err, "setup：写入配置失败");
        return fail("配置无法写入磁盘，请检查服务对该目录的写权限。");
    }

    tracing::info!(
        database = %sloray_config::mask_database_url(db_url),
        "setup 完成，实例已配置"
    );

    // ---- 7) 自动登录 ----
    let secure = auth::cookie_is_secure(updated.public_site_url.as_deref());
    match created_admin {
        Some(user) => match auth::create_session(&db, user.id, None).await {
            Ok(token) => {
                redirect_with_cookie("/admin/settings", auth::session_cookie(&token, secure))
            }
            // 建会话失败不算致命：配置已经生效，让他手动登录即可。
            Err(err) => {
                tracing::warn!(error = %err, "setup：自动登录失败");
                Redirect::to("/login").into_response()
            }
        },
        None => Redirect::to("/login").into_response(),
    }
}
