//! `sessions` 表。
//!
//! 对应 `crates/migration/src/m20261007_000001_init.rs` 里的 DDL。
//!
//! 关键设计（ADR-0007）：**库里只存 token 的 SHA-256，不存明文**。
//! 数据库被读走不等于会话被接管。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "sessions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// SHA-256(token) 的原始字节，数据库上是 `bytea` 且有唯一索引。
    pub token_hash: Vec<u8>,
    pub user_id: Uuid,
    pub expires_at: DateTimeWithTimeZone,
    /// 客户端 IP 的哈希（不存明文 IP，减少隐私面）。
    pub ip_hash: Option<Vec<u8>>,
    pub user_agent: Option<String>,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
