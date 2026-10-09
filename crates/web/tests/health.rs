//! 端到端 HTTP 测试，**不需要真实 Postgres**。
//!
//! 用 `ConnectOptions::connect_lazy(true)` 构造一个"永远连不上"的连接池，
//! 于是可以在不装数据库的机器上验证状态机最关键的部分：
//! 未配置 / 配了但连不上 / 依赖抖动时，探针与页面各自如何表现。
//!
//! 需要真实数据库的路径（setup 提交、登录、改配置）由
//! `deploy/scripts/` 之外的端到端冒烟覆盖，见 docs/deployment.md。

use std::time::Duration;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use sea_orm::{ConnectOptions, Database};
use sloray_config::Config;
use sloray_web::{AppState, routes};
use tower::ServiceExt;

/// 未配置：没有数据库连接串。这是刚装好的实例的状态。
fn unconfigured_state() -> AppState {
    AppState::new(Config::default(), None)
}

/// 已配置，但指向一个必然不可达的地址。
/// `connect_lazy` 让连接池的构造不会真的去连。
async fn configured_but_unreachable_state() -> AppState {
    let mut options =
        ConnectOptions::new("postgres://sloray:invalid@127.0.0.1:1/sloray".to_owned());
    options.connect_lazy(true);
    // 缩短获取连接的等待：readiness 探测应该快速失败，
    // 而不是把测试卡在默认的 30 秒上（生产环境配的是 5 秒）。
    options.acquire_timeout(Duration::from_secs(1));

    let db = Database::connect(options)
        .await
        .expect("lazy 连接不应该在建立连接池时失败");

    let config = Config {
        database_url: Some("postgres://sloray:invalid@127.0.0.1:1/sloray".into()),
        ..Config::default()
    };

    AppState::new(config, Some(db))
}

async fn send(app: Router, uri: &str) -> (StatusCode, String) {
    let response = app
        .oneshot(
            Request::builder()
                .uri(uri)
                .body(Body::empty())
                .expect("请求构造失败"),
        )
        .await
        .expect("路由应当返回响应");

    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("读取响应体失败")
        .to_bytes();

    (status, String::from_utf8_lossy(&bytes).into_owned())
}

async fn get_json(app: Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let (status, text) = send(app, uri).await;
    let json = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
    (status, json)
}

// ---------------------------------------------------------------- 探针

#[tokio::test]
async fn liveness_is_ok_even_when_completely_unconfigured() {
    // 最重要的一条：刚装好、什么都还没配时，liveness 必须是 200。
    // 否则编排层会陷入"重启一个本来健康的进程"的循环，用户永远打不开 setup 页。
    let (status, json) = get_json(routes::build(unconfigured_state()), "/healthz").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "ok");
}

#[tokio::test]
async fn readiness_reports_not_configured_before_setup() {
    let (status, json) = get_json(routes::build(unconfigured_state()), "/readyz").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(json["status"], "degraded");
    assert_eq!(json["checks"][0]["name"], "postgres");
    assert_eq!(json["checks"][0]["ok"], false);
    // 用固定文案区分"没配"与"连不上"，运维一眼能看出该做什么。
    assert_eq!(json["checks"][0]["detail"], "not configured");
}

#[tokio::test]
async fn readiness_reports_503_when_configured_but_database_is_down() {
    let state = configured_but_unreachable_state().await;
    let (status, json) = get_json(routes::build(state), "/readyz").await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(json["checks"][0]["ok"], false);
    assert_eq!(json["checks"][0]["detail"], "database unavailable");
}

#[tokio::test]
async fn probes_do_not_leak_connection_details() {
    // 探针对公网开放，所以响应体里绝不能出现连接串、主机名或 SQL。
    for state in [
        unconfigured_state(),
        configured_but_unreachable_state().await,
    ] {
        let (_, text) = send(routes::build(state), "/readyz").await;
        assert!(!text.contains("sloray:invalid"), "泄漏了连接串: {text}");
        assert!(!text.contains("127.0.0.1"), "泄漏了主机信息: {text}");
        assert!(
            !text.to_lowercase().contains("select"),
            "泄漏了 SQL: {text}"
        );
        assert!(!text.contains("password"), "泄漏了凭据相关字样: {text}");
    }
}

#[tokio::test]
async fn status_endpoint_exposes_setup_mode_without_secrets() {
    let (status, json) = get_json(routes::build(unconfigured_state()), "/api/v1/status").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["configured"], false);
    assert_eq!(json["setup_mode"], "required");
    assert_eq!(json["database_reachable"], false);
    assert!(
        !json.to_string().contains("postgres://"),
        "状态端点不应回显连接串"
    );

    let state = configured_but_unreachable_state().await;
    let (_, json) = get_json(routes::build(state), "/api/v1/status").await;
    assert_eq!(json["configured"], true);
    // 配了但连不上 → 走救急通道，这一点必须如实反映出来。
    assert_eq!(json["setup_mode"], "recoverable");
}

// ---------------------------------------------------------------- 页面

#[tokio::test]
async fn setup_page_is_reachable_when_unconfigured() {
    // 这是整套"WordPress 式部署流程"的前提：什么依赖都没有时，
    // /setup 必须能打开。
    let (status, body) = send(routes::build(unconfigured_state()), "/setup").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("<form"), "应当是表单页，实际：{body}");
    assert!(body.contains("name=\"token\""), "缺少 token 输入框");
    assert!(body.contains("name=\"db_url\""), "缺少数据库连接串输入框");
}

#[tokio::test]
async fn setup_page_stays_open_when_database_is_unreachable() {
    // 写错数据库密码时，唯一的恢复途径就是这个页面 —— 它必须还在。
    let state = configured_but_unreachable_state().await;
    let (status, body) = send(routes::build(state), "/setup").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("救急通道") || body.contains("连不上"));
}

#[tokio::test]
async fn settings_requires_login() {
    let (status, _) = send(routes::build(unconfigured_state()), "/admin/settings").await;
    // 没登录 → 重定向到 /login，而不是 500 或空白页。
    assert!(status.is_redirection(), "实际状态码：{status}");
}

#[tokio::test]
async fn login_redirects_to_setup_before_configuration() {
    // 还没配数据库时无法校验密码，所以登录页应当把人送回 setup。
    let (status, _) = send(routes::build(unconfigured_state()), "/login").await;
    assert!(status.is_redirection(), "实际状态码：{status}");
}

// ---------------------------------------------------------------- 基本契约

#[tokio::test]
async fn ping_is_reachable_under_the_versioned_prefix() {
    let (status, json) = get_json(routes::build(unconfigured_state()), "/api/v1/ping").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["pong"], true);
}

#[tokio::test]
async fn unknown_route_uses_the_standard_error_envelope() {
    let (status, json) = get_json(
        routes::build(unconfigured_state()),
        "/definitely-not-a-route",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"]["code"], "not_found");
    assert!(json["error"]["message"].is_string());
}
