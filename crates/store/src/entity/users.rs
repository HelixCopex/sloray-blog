//! `users` 表。
//!
//! 对应 `crates/migration/src/m20261007_000001_init.rs` 里的 DDL：
//!
//! ```sql
//! CREATE TABLE users (
//!     id uuid PRIMARY KEY,
//!     email text NOT NULL UNIQUE,
//!     password_hash text NOT NULL,
//!     display_name text NOT NULL,
//!     role text NOT NULL,
//!     created_at timestamptz NOT NULL DEFAULT now(),
//!     updated_at timestamptz NOT NULL DEFAULT now(),
//!     CONSTRAINT users_role_valid CHECK (role IN ('admin', 'author')),
//!     CONSTRAINT users_email_folded CHECK (email = lower(email))
//! );
//! ```

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    /// UUIDv7，由应用层生成（不用数据库序列，见架构文档的分布式就绪清单）。
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// 数据库上有 `CHECK (email = lower(email))`，写入前必须已转小写。
    pub email: String,
    /// argon2id 的 PHC 字符串（含 salt 与参数），**不是**明文，也不是可逆加密。
    pub password_hash: String,
    pub display_name: String,
    /// 'admin' | 'author'，由数据库的 CHECK 约束保证取值合法。
    pub role: String,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

/// 角色常量。数据库有 CHECK 约束兜底，这里只是避免代码里散落魔法字符串。
pub const ROLE_ADMIN: &str = "admin";

impl Model {
    pub fn is_admin(&self) -> bool {
        self.role == ROLE_ADMIN
    }
}
