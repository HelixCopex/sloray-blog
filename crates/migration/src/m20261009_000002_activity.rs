//! Expand：跨实例限流与匿名地址脱敏盐；不改变旧表的读写契约。
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for sql in [
            "CREATE TABLE privacy_secrets (name text PRIMARY KEY, value text NOT NULL)",
            "CREATE TABLE request_limits (scope text NOT NULL, subject bytea NOT NULL, bucket bigint NOT NULL, count bigint NOT NULL, expires_at timestamptz NOT NULL, PRIMARY KEY(scope,subject,bucket))",
            "CREATE INDEX request_limits_expiry_idx ON request_limits(expires_at)",
            "CREATE INDEX posts_schedule_idx ON posts(published_at,id) WHERE status='scheduled' AND deleted_at IS NULL",
        ] {
            manager.get_connection().execute_unprepared(sql).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for sql in [
            "DROP INDEX posts_schedule_idx",
            "DROP TABLE request_limits",
            "DROP TABLE privacy_secrets",
        ] {
            manager.get_connection().execute_unprepared(sql).await?;
        }
        Ok(())
    }
}
