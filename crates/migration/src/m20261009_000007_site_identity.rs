use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for sql in [
            "CREATE TABLE site_identity(id integer PRIMARY KEY CHECK(id=1),site_name text NOT NULL DEFAULT '静线数据库',favicon_url text,version integer NOT NULL DEFAULT 1 CHECK(version>0))",
            "INSERT INTO site_identity(id) VALUES(1)",
            "CREATE FUNCTION sloray_identity_purge() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN INSERT INTO outbox(id,topic,payload) VALUES(gen_random_uuid(),'cdn.purge',jsonb_build_object('paths',(SELECT jsonb_agg(path) FROM (SELECT '/' AS path UNION SELECT '/rss.xml' UNION SELECT '/sitemap.xml' UNION SELECT '/posts/'||slug FROM posts WHERE status='published' AND deleted_at IS NULL UNION SELECT '/'||slug FROM pages WHERE status='published' AND deleted_at IS NULL) paths))); RETURN NEW; END $$",
            "CREATE TRIGGER site_identity_purge AFTER UPDATE ON site_identity FOR EACH ROW EXECUTE FUNCTION sloray_identity_purge()",
        ] {
            manager.get_connection().execute_unprepared(sql).await?;
        }
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE site_identity")
            .await?;
        manager
            .get_connection()
            .execute_unprepared("DROP FUNCTION sloray_identity_purge()")
            .await?;
        Ok(())
    }
}
