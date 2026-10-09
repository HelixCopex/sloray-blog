//! 数据访问 crate：SeaORM 实体与连接池。
//!
//! 只负责"怎么连、表长什么样"，不含任何业务规则 ——
//! 业务规则散进这一层，是这类项目最常见的腐化方式。

#![forbid(unsafe_code)]

pub mod entity;

use std::time::Duration;

use sea_orm::{ConnectOptions, Database, DatabaseConnection};

/// 建立数据库连接池。
///
/// 注意这里**不跑迁移**：迁移由 `sloray migrate` 在部署流程里执行，
/// 或在 setup 向导里带咨询锁单飞执行一次。让 `serve` 每次启动都跑迁移，
/// 会在多实例滚动发布时产生并发竞争（ADR-0006）。
pub async fn connect_database(
    url: &str,
    config: &sloray_config::Config,
) -> std::result::Result<DatabaseConnection, sea_orm::DbErr> {
    let mut options = ConnectOptions::new(url.to_owned());
    options
        .max_connections(config.database_max_connections)
        .min_connections(1)
        .acquire_timeout(Duration::from_secs(config.database_acquire_timeout_secs))
        // SQL 日志交给 tracing 层，避免 sqlx 自己再打一份。
        .sqlx_logging(false)
        .set_application_name("sloray");

    Database::connect(options).await
}
