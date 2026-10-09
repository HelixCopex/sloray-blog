//! Expand：可由管理员维护的独立页面，不改变文章与历史迁移。
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for sql in [
            "CREATE TABLE pages (id uuid PRIMARY KEY, slug text NOT NULL UNIQUE, title text NOT NULL, summary text NOT NULL DEFAULT '', body_md text NOT NULL, status text NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','published')), show_in_nav boolean NOT NULL DEFAULT true, nav_order integer NOT NULL DEFAULT 0, version integer NOT NULL DEFAULT 1 CHECK(version>0), created_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT now(), deleted_at timestamptz)",
            "CREATE INDEX pages_listing_idx ON pages(nav_order,id) WHERE deleted_at IS NULL",
            "INSERT INTO pages(id,slug,title,summary,body_md,status) VALUES(gen_random_uuid(),'about','关于','关于静线数据库','这是一个前后端分离的动态博客：Rust 负责数据与业务，Astro 负责呈现，Postgres 是唯一权威源，媒体走对象存储。','published')",
            "CREATE FUNCTION sloray_page_purge() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN INSERT INTO outbox(id,topic,payload) VALUES(gen_random_uuid(),'cdn.purge',jsonb_build_object('paths',jsonb_build_array('/','/sitemap.xml','/'||NEW.slug))); IF TG_OP='UPDATE' AND OLD.slug<>NEW.slug THEN INSERT INTO outbox(id,topic,payload) VALUES(gen_random_uuid(),'cdn.purge',jsonb_build_object('paths',jsonb_build_array('/'||OLD.slug))); END IF; RETURN NEW; END $$",
            "CREATE TRIGGER pages_purge_trigger AFTER INSERT OR UPDATE ON pages FOR EACH ROW EXECUTE FUNCTION sloray_page_purge()",
        ] {
            manager.get_connection().execute_unprepared(sql).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for sql in ["DROP TABLE pages", "DROP FUNCTION sloray_page_purge()"] {
            manager.get_connection().execute_unprepared(sql).await?;
        }
        Ok(())
    }
}
