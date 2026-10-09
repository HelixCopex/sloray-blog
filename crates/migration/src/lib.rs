//! 数据库迁移。
//!
//! 迁移由 `sloray migrate` 子命令在部署流程里执行一次；
//! **也可以**由 setup 向导在执行首次配置时调用一次。
//! 两条路径都走 [`migrate_up_with_lock`]，靠 Postgres 的事务级咨询锁保证单飞。
//!
//! 为什么不在每次 `serve` 启动时跑迁移：那会让多实例滚动发布产生并发迁移竞争
//! （ADR-0006）。setup 向导是例外 —— 那是**一次性的初始化**，不是常规启动路径，
//! 而且咨询锁让它即使被并发触发也是安全的。
//!
//! 本 crate 的 DDL 刻意使用 Postgres 原生 SQL（见 `m20261007_000001_init.rs` 的说明）。

#![forbid(unsafe_code)]

pub use sea_orm_migration::prelude::*;

mod m20261007_000001_init;
mod m20261009_000002_activity;
mod m20261009_000003_search;
mod m20261009_000004_media;
mod m20261009_000005_pages;
mod m20261009_000006_themes;
mod m20261009_000007_site_identity;
mod m20261010_000008_page_dock_icon;

/// 迁移用的咨询锁 key：`0x736C6F726179` 就是 ASCII 的 "sloray"。
///
/// 用 `pg_advisory_xact_lock`（事务级）而不是 `pg_advisory_lock`（会话级）：
/// 会话级锁绑定在某一条连接上，而连接来自池、可能换手；
/// 事务级锁在事务结束时自动释放，进程崩溃也不会留下死锁。
pub const MIGRATION_LOCK_KEY: i64 = 0x736C_6F72_6179;

/// 迁移清单。**只追加，不修改已发布的迁移**。
pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20261007_000001_init::Migration),
            Box::new(m20261009_000002_activity::Migration),
            Box::new(m20261009_000003_search::Migration),
            Box::new(m20261009_000004_media::Migration),
            Box::new(m20261009_000005_pages::Migration),
            Box::new(m20261009_000006_themes::Migration),
            Box::new(m20261009_000007_site_identity::Migration),
            Box::new(m20261010_000008_page_dock_icon::Migration),
        ]
    }
}

/// 在咨询锁保护下应用全部迁移。
///
/// 整个迁移过程包在一个事务里：拿到锁 → 跑迁移 → 提交。
/// 这样两个并发调用（例如两个 API 实例同时在 setup，或部署脚本与向导撞上）
/// 不会同时改 schema —— 后到的那个会等前一个提交后再看到"无待执行迁移"。
pub async fn migrate_up_with_lock(db: &sea_orm::DatabaseConnection) -> anyhow::Result<()> {
    migrate_with_lock(db, Operation::Up(None)).await
}

pub enum Operation {
    Up(Option<u32>),
    Down(u32),
    Fresh,
}

/// 所有会修改 schema 的 CLI 路径使用同一把锁，包括带步数的 up/down/fresh。
pub async fn migrate_with_lock(
    db: &sea_orm::DatabaseConnection,
    operation: Operation,
) -> anyhow::Result<()> {
    use sea_orm::{ConnectionTrait, TransactionTrait};

    let txn = db.begin().await?;

    txn.execute_unprepared(&format!(
        "SELECT pg_advisory_xact_lock({MIGRATION_LOCK_KEY})"
    ))
    .await?;

    let result = match operation {
        Operation::Up(steps) => Migrator::up(&txn, steps).await,
        Operation::Down(steps) => Migrator::down(&txn, Some(steps)).await,
        Operation::Fresh => {
            // SeaORM fresh 只清表；先逆序 down 清除我们声明的函数/触发器等对象。
            // 不能以 CREATE OR REPLACE 掩盖遗留对象，历史迁移也保持不变。
            let steps = Migrator::get_applied_migrations(&txn).await?.len() as u32;
            Migrator::down(&txn, Some(steps)).await?;
            Migrator::fresh(&txn).await
        }
    };
    match result {
        Ok(()) => {
            txn.commit().await?;
            Ok(())
        }
        Err(err) => {
            // 显式回滚，把失败原因原样抛出。
            let _ = txn.rollback().await;
            Err(err.into())
        }
    }
}
