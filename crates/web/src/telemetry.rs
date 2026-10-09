//! tracing 初始化。日志是这台单机 VPS 上最主要的可观测手段，别省。
//!
//! **幂等**：这里用 `try_init` 而不是 `init`。
//! 原因不是"优雅"，而是一个真实的坑：CLI 侧与 `server::run` 都会初始化日志，
//! 用 `init` 时第二次调用会直接 panic（全局 subscriber 只能设置一次），
//! 表现是"进程起来就死，日志里只有一句 SetGlobalDefaultError"。
//! 把幂等性放在这里，调用方就不必记住"只能调一次"这条隐式约定。

use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use sloray_config::{Config, LogFormat};

/// 默认过滤规则：自己 debug，框架 info，压掉 sqlx 的噪音。
const DEFAULT_FILTER: &str = "info,sloray=debug,sloray_api=debug,tower_http=info,sea_orm=info";

/// 按配置初始化（JSON / pretty）。重复调用是安全的，第一次生效。
pub fn init(config: &Config) {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));

    let registry = tracing_subscriber::registry().with(filter);

    let result = match config.log_format {
        LogFormat::Json => registry
            .with(
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_current_span(true)
                    .with_span_list(false),
            )
            .try_init(),
        LogFormat::Pretty => registry.with(tracing_subscriber::fmt::layer()).try_init(),
    };

    // 已经初始化过就静默忽略 —— 这是正常路径（CLI 与 server::run 都会调），
    // 不是错误，不该往 stderr 打字。
    let _ = result;
}

/// 给不需要读配置的子命令用（`systemd` / `setup-token`）。
///
/// 这些命令要能在"配置文件根本读不出来"的时候工作 ——
/// 否则你就无法用 `systemd install` 去修一个坏掉的部署。
pub fn init_default() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .try_init();
}
