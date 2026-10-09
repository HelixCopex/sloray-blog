//! HTTP 服务的启动与优雅退出。
//!
//! 与早期版本最大的差别：**数据库连不上不再阻止启动**。
//!
//! 这不是放松要求，而是这套部署流程的前提：如果配置写错就起不来，
//! 那"打开网页改配置"这条路径在最需要它的时候就失效了 ——
//! 你只能 SSH 上去手改文件，而那正是我们要消灭的操作。
//! 所以此时服务照常监听，`/readyz` 如实返回 503，
//! `/setup` 提供救急通道。

use std::time::Duration;

use tokio::net::TcpListener;
use tracing::{info, warn};

use sloray_config::Config;
use sloray_setup as setup;
use sloray_store::connect_database;

use crate::{routes, state::AppState, telemetry};

pub async fn run(config: Config) -> anyhow::Result<()> {
    telemetry::init(&config);
    config.warn_on_risky_defaults();

    // ---- 尝试连接数据库；失败只降级，不退出 ----
    let db = match config.database_url.as_deref() {
        Some(url) => match connect_database(url, &config).await {
            Ok(db) => Some(db),
            Err(err) => {
                warn!(
                    error = %err,
                    "数据库连接失败：进入降级状态（/readyz 返回 503，/setup 可用来改配置）"
                );
                None
            }
        },
        None => {
            info!("尚未配置数据库：进入 setup 模式");
            None
        }
    };

    // ---- 准备 setup token ----
    // 只在"还不能正常工作"时提示，避免每次重启都把 token 刷进日志。
    let token_path = setup::token_path(&config.config_file);
    match setup::load_or_create_token(&token_path) {
        Ok(token) => {
            if db.is_none() {
                info!(token_file = %token_path.display(), "初始配置尚未完成");
                // 单独一行、固定前缀，方便 `journalctl | grep 'SETUP TOKEN'`。
                info!("SLORAY SETUP TOKEN: {token}");
                info!("打开 http://<这个地址>/setup 并填入上面的 token 完成初始化");
            }
        }
        Err(err) => warn!(error = %err, "无法准备 setup token，/setup 将不可用"),
    }

    // ---- 监听 ----
    let drain_timeout = Duration::from_secs(config.shutdown_timeout_secs);
    let bind_addr = config.bind_addr;
    let environment = config.environment;
    // 在把 config 移进 AppState 之前先取出来，日志里要用。
    let configured = config.is_configured();

    let state = AppState::new(config, db);
    let refresh_state = state.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(2));
        loop {
            tick.tick().await;
            if let Err(err) = refresh_state.refresh().await {
                tracing::debug!(error = %err, "配置刷新或数据库重连尚未成功");
            }
        }
    });
    let app = routes::build(state);

    let listener = TcpListener::bind(bind_addr)
        .await
        .map_err(|e| anyhow::anyhow!("无法监听 {bind_addr}：{e}"))?;

    info!(
        %bind_addr,
        ?environment,
        version = env!("CARGO_PKG_VERSION"),
        configured,
        "sloray 已启动"
    );

    // 用 oneshot 把"收到信号"传给 serve；再用看门狗给 drain 加上时限。
    // 没有时限的话，一个卡住的请求会让进程永远退不出去，systemd 只能 SIGKILL。
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let server = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        // 发送方被 drop（例如看门狗分支先结束）也应视为需要关闭。
        let _ = shutdown_rx.await;
    });

    let watchdog = async move {
        shutdown_signal().await;
        info!(
            timeout_secs = drain_timeout.as_secs(),
            "收到关闭信号，开始 drain 在途请求"
        );
        let _ = shutdown_tx.send(());
        tokio::time::sleep(drain_timeout).await;
    };

    tokio::select! {
        result = server => {
            result.map_err(|e| anyhow::anyhow!("HTTP 服务异常退出：{e}"))?;
            info!("已完成 drain，正常退出");
        }
        () = watchdog => {
            warn!(
                timeout_secs = drain_timeout.as_secs(),
                "在途请求未在超时内结束，强制退出"
            );
        }
    }

    Ok(())
}

/// 等待 SIGTERM 或 Ctrl-C。
///
/// SIGTERM 是 systemd / 部署脚本使用的信号；Ctrl-C 供本地开发使用。
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("注册 Ctrl-C 处理器失败");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("注册 SIGTERM 处理器失败")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}
