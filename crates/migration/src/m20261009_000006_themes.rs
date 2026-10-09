use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for sql in [
            "CREATE TABLE themes(id uuid PRIMARY KEY,package jsonb NOT NULL,version integer NOT NULL DEFAULT 1 CHECK(version>0),updated_at timestamptz NOT NULL DEFAULT now(),deleted_at timestamptz)",
            "CREATE TABLE theme_settings(id integer PRIMARY KEY CHECK(id=1),active_theme_id uuid REFERENCES themes(id),package jsonb,version integer NOT NULL DEFAULT 1 CHECK(version>0),CHECK((active_theme_id IS NULL)=(package IS NULL)))",
            "INSERT INTO theme_settings(id) VALUES(1)",
            "CREATE FUNCTION sloray_theme_purge() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN INSERT INTO outbox(id,topic,payload) VALUES(gen_random_uuid(),'cdn.purge',jsonb_build_object('paths',(SELECT jsonb_agg(path) FROM (SELECT '/' AS path UNION SELECT '/sitemap.xml' UNION SELECT '/posts/'||slug FROM posts WHERE status='published' AND deleted_at IS NULL UNION SELECT '/'||slug FROM pages WHERE status='published' AND deleted_at IS NULL) paths))); RETURN NEW; END $$",
            "CREATE TRIGGER theme_purge_trigger AFTER UPDATE ON theme_settings FOR EACH ROW EXECUTE FUNCTION sloray_theme_purge()",
        ] {
            manager.get_connection().execute_unprepared(sql).await?;
        }
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for sql in [
            "DROP TABLE theme_settings",
            "DROP TABLE themes",
            "DROP FUNCTION sloray_theme_purge()",
        ] {
            manager.get_connection().execute_unprepared(sql).await?;
        }
        Ok(())
    }
}
