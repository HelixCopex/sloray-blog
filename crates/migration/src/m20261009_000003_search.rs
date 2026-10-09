//! Expand：权威原文保持不变，新增可丢弃的中文检索派生列。
use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;

const UP: &[&str] = &[
    "CREATE TABLE search_settings (singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton), engine text NOT NULL CHECK(engine IN ('database','app')), config text NOT NULL)",
    "DO $$ DECLARE cfg text; BEGIN
        FOREACH cfg IN ARRAY ARRAY['public.jiebacfg','public.zhcfg'] LOOP
            IF EXISTS(SELECT 1 FROM pg_ts_config c JOIN pg_namespace n ON n.oid=c.cfgnamespace WHERE n.nspname=split_part(cfg,'.',1) AND c.cfgname=split_part(cfg,'.',2)) THEN
                IF to_tsvector(cfg::regconfig,'分布式博客架构') @@ websearch_to_tsquery(cfg::regconfig,'博客') THEN
                    INSERT INTO search_settings(engine,config) VALUES('database',cfg); RETURN;
                END IF;
            END IF;
        END LOOP;
        INSERT INTO search_settings(engine,config) VALUES('app','pg_catalog.simple');
    END $$",
    "ALTER TABLE posts ADD COLUMN search_title text, ADD COLUMN search_summary text, ADD COLUMN search_body text, ADD COLUMN search_source text, ADD COLUMN search_vector tsvector",
    "CREATE FUNCTION sloray_posts_search() RETURNS trigger LANGUAGE plpgsql AS $$
        DECLARE cfg regconfig; mode text;
        BEGIN
            SELECT config::regconfig,engine INTO cfg,mode FROM search_settings;
            IF mode='database' THEN
                NEW.search_vector := setweight(to_tsvector(cfg,NEW.title),'A') || setweight(to_tsvector(cfg,NEW.summary),'B') || setweight(to_tsvector(cfg,NEW.body_md),'C');
            ELSIF NEW.search_source = md5(jsonb_build_array(NEW.title,NEW.summary,NEW.body_md)::text) THEN
                NEW.search_vector := setweight(to_tsvector(cfg,coalesce(NEW.search_title,'')),'A') || setweight(to_tsvector(cfg,coalesce(NEW.search_summary,'')),'B') || setweight(to_tsvector(cfg,coalesce(NEW.search_body,'')),'C');
            ELSE NEW.search_vector := NULL;
            END IF;
            RETURN NEW;
        END $$",
    "CREATE TRIGGER posts_search_trigger BEFORE INSERT OR UPDATE OF title,summary,body_md,search_title,search_summary,search_body,search_source ON posts FOR EACH ROW EXECUTE FUNCTION sloray_posts_search()",
    "UPDATE posts SET title=title",
    "CREATE INDEX posts_search_idx ON posts USING GIN(search_vector) WHERE deleted_at IS NULL",
    "CREATE FUNCTION rebuild_posts_search_vector() RETURNS bigint LANGUAGE plpgsql AS $$
        DECLARE affected bigint; mode text;
        BEGIN
            SELECT engine INTO mode FROM search_settings;
            IF mode<>'database' THEN RAISE EXCEPTION 'application segmentation requires sloray rebuild-search'; END IF;
            UPDATE posts SET title=title WHERE deleted_at IS NULL;
            GET DIAGNOSTICS affected = ROW_COUNT;
            RETURN affected;
        END $$",
];

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for sql in UP {
            manager.get_connection().execute_unprepared(sql).await?;
        }
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for sql in [
            "DROP FUNCTION rebuild_posts_search_vector()",
            "DROP INDEX posts_search_idx",
            "DROP TRIGGER posts_search_trigger ON posts",
            "DROP FUNCTION sloray_posts_search()",
            "ALTER TABLE posts DROP COLUMN search_vector,DROP COLUMN search_source,DROP COLUMN search_body,DROP COLUMN search_summary,DROP COLUMN search_title",
            "DROP TABLE search_settings",
        ] {
            manager.get_connection().execute_unprepared(sql).await?;
        }
        Ok(())
    }
}
