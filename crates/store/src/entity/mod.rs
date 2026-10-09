//! SeaORM 实体。
//!
//! 目前只需要 `users` 与 `sessions` 两张表 —— setup 向导要创建管理员，
//! 配置页要做会话鉴权。其余表等各自里程碑再补。
//!
//! **实体必须与 `crates/migration` 里的 DDL 保持一致**：改一边必须改另一边。
//! 两者的一致性由 CI 的 `migrations` job（真实 Postgres 上跑迁移）
//! 加上端到端的 setup 冒烟测试来保证。

pub mod sessions;
pub mod users;
