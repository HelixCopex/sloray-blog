//! 健康检查。
//!
//! 刻意分成两个端点，这是滚动重启与蓝绿发布的基础：
//!
//! * `/healthz`（liveness）—— 进程还活着吗？**不检查依赖**。
//!   若它检查数据库，Postgres 抖一下就会让 systemd 反复重启一个本来健康的进程。
//! * `/readyz`（readiness）—— 现在能接流量吗？检查依赖，失败返回 503。
//!   发布脚本与 Nginx 的健康检查靠它决定何时切流量。
//!
//! 两者**对公网开放**：外部探测系统可以直接用，不必开 SSH 隧道。
//! 敢公开的依据是响应体已经脱敏 —— 只有状态与依赖名，
//! 不含连接串、主机名或 SQL。这一点由 `tests/health.rs` 守护。

use axum::{Json, extract::State, http::StatusCode};
use serde::Serialize;

use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct Liveness {
    pub status: &'static str,
    pub version: &'static str,
}

/// liveness：只要进程能响应就返回 200。
pub async fn livez() -> Json<Liveness> {
    Json(Liveness {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

#[derive(Debug, Serialize)]
pub struct Readiness {
    pub status: &'static str,
    pub checks: Vec<Check>,
}

#[derive(Debug, Serialize)]
pub struct Check {
    pub name: &'static str,
    pub ok: bool,
    /// 给运维看的简短原因；**不含**连接串、SQL 或任何凭据。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// readiness：探测 Postgres。未配置或连不上都返回 503。
pub async fn readyz(State(state): State<AppState>) -> (StatusCode, Json<Readiness>) {
    let postgres = match state.db_optional().await {
        // 还没配置：这不是"坏了"，但确实还不能接流量。
        // detail 用固定文案，不带任何环境细节。
        None => Check {
            name: "postgres",
            ok: false,
            detail: Some("not configured".to_owned()),
        },
        Some(db) => match db.ping().await {
            Ok(()) => Check {
                name: "postgres",
                ok: true,
                detail: None,
            },
            Err(err) => {
                // 细节只进日志，不进响应体。
                tracing::warn!(error = %err, "readiness 探测失败：postgres 不可用");
                Check {
                    name: "postgres",
                    ok: false,
                    detail: Some("database unavailable".to_owned()),
                }
            }
        },
    };

    let ready = postgres.ok;
    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    let body = Readiness {
        status: if ready { "ok" } else { "degraded" },
        checks: vec![postgres],
    };

    (status, Json(body))
}
