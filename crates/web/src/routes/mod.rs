pub mod activity;
pub mod content;
pub mod health;
pub mod media;
pub mod pages;
mod protection;
pub mod search;
pub mod session;
pub mod site;
pub mod site_pages;
pub mod users;

use axum::{
    Json, Router,
    extract::{Request, State},
    middleware::{self, Next},
    response::Response,
    routing::get,
};
use serde::Serialize;
use tower::ServiceBuilder;
use tower_http::{
    catch_panic::CatchPanicLayer,
    compression::CompressionLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};

use crate::{AppError, state::AppState};

/// 组装完整的路由与中间件栈。
///
/// 路径布局（与 `deploy/nginx/sloray.conf` 的分流规则必须保持一致）：
///
/// * `/healthz`、`/readyz` —— 基础设施探针，**对公网开放**（响应体已脱敏）。
/// * `/setup` —— 首次配置向导。由 **Rust 自己渲染 HTML**，不依赖 Astro/Nginx/域名：
///   它必须在"什么都还没配好"的时刻可用。仅在未配置或数据库不可达时响应。
/// * `/login`、`/logout`、`/admin/settings` —— 同样由 Rust 自渲染，
///   用于登录与运行时改配置。
/// * `/api/v1/*` —— 对外 JSON API。
pub fn build(state: AppState) -> Router {
    Router::new()
        .merge(content::router())
        .merge(site_pages::router())
        .merge(activity::router())
        .merge(media::router())
        .route("/api/v1/site", get(site::site))
        .route("/api/v1/admin/users", get(users::list).post(users::create))
        .route("/api/v1/search", get(search::search))
        .route("/api/v1/auth/session", get(session::session))
        .route(
            "/api/v1/openapi.json",
            get(|| async { Json(crate::openapi::document()) }),
        )
        .route("/healthz", get(health::livez))
        .route("/readyz", get(health::readyz))
        .route("/api/v1/ping", get(ping))
        .route("/api/v1/status", get(status))
        .route("/setup", get(pages::setup_form).post(pages::setup_submit))
        .route("/login", get(pages::login_form).post(pages::login_submit))
        .route("/logout", axum::routing::post(pages::logout))
        .route(
            "/admin/settings",
            get(pages::settings_form).post(pages::settings_submit),
        )
        .fallback(not_found)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            protection::protect,
        ))
        .with_state(state)
        .layer(middleware::from_fn(response_headers))
        .layer(
            ServiceBuilder::new()
                // 请求 ID 必须在 TraceLayer 之前注入，否则日志里没有它。
                .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
                .layer(TraceLayer::new_for_http())
                // 回写 X-Request-Id，方便把浏览器报错和日志对上。
                .layer(PropagateRequestIdLayer::x_request_id())
                .layer(CompressionLayer::new())
                // 单个 handler panic 不应该带走整个进程。
                .layer(CatchPanicLayer::new()),
        )
}

async fn response_headers(request: Request, next: Next) -> Response {
    use axum::http::{HeaderValue, header};
    let api = request.uri().path().starts_with("/api/");
    let mut response = next.run(request).await;
    // Json / Query / Path 等 extractor 与方法拒绝也遵守统一信封；不回显解析内部细节。
    if api
        && response.status().is_client_error()
        && !response
            .headers()
            .get(header::CONTENT_TYPE)
            .is_some_and(|v| v.as_bytes().starts_with(b"application/json"))
    {
        use axum::response::IntoResponse;
        let status = response.status();
        response = (
            status,
            Json(serde_json::json!({"error":{"code":"bad_request","message":"invalid request"}})),
        )
            .into_response();
    }
    response
        .headers_mut()
        .entry(header::CACHE_CONTROL)
        .or_insert(HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

#[derive(Debug, Serialize)]
struct Pong {
    pong: bool,
    version: &'static str,
}

/// 最小连通性探针：用于验证「Nginx 同源分流 → Rust API」这条链路是通的。
async fn ping() -> Json<Pong> {
    Json(Pong {
        pong: true,
        version: env!("CARGO_PKG_VERSION"),
    })
}

#[derive(Debug, Serialize)]
struct Status {
    version: &'static str,
    /// 是否已完成初始配置。
    configured: bool,
    /// `required` / `recoverable` / `disabled`
    setup_mode: &'static str,
    /// 数据库当前是否可用。
    database_reachable: bool,
}

/// 给前端与运维用的状态汇总。
///
/// 刻意**不含**任何连接串、主机名或凭据 —— 它是公开端点。
async fn status(State(state): State<AppState>) -> Json<Status> {
    let config = state.config().await;
    let reachable = match state.db_optional().await {
        Some(db) => db.ping().await.is_ok(),
        None => false,
    };
    let mode = sloray_setup::database_mode(&config, state.db_optional().await.as_ref()).await;

    Json(Status {
        version: env!("CARGO_PKG_VERSION"),
        configured: config.is_configured(),
        setup_mode: mode.as_str(),
        database_reachable: reachable,
    })
}

/// 未匹配到路由时也返回统一错误信封，而不是 axum 默认的空 body。
async fn not_found() -> AppError {
    AppError::NotFound
}
