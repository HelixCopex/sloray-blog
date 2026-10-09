//! Expand：独立页面的受控 Dock 图标，旧二进制仍可读写 pages。
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(
            "ALTER TABLE pages ADD COLUMN dock_icon text NOT NULL DEFAULT 'article' CHECK (dock_icon IN ('article','home','person','image','reading','comment','tag','rss','preview','palette','settings','edit'))"
        ).await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE pages DROP COLUMN dock_icon")
            .await?;
        Ok(())
    }
}
