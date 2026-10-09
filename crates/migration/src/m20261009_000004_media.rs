//! Expand：上传意图、软删除与 outbox 重试状态。
use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;

const UP: &[&str] = &[
    "ALTER TABLE media ADD COLUMN variants_generated_at timestamptz, ADD COLUMN deleted_at timestamptz, ADD COLUMN verified boolean NOT NULL DEFAULT false",
    "CREATE TABLE media_uploads (id uuid PRIMARY KEY, owner_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE, object_key text NOT NULL UNIQUE, mime text NOT NULL, size_bytes bigint NOT NULL CHECK(size_bytes>0 AND size_bytes<=10485760), sha256 bytea NOT NULL CHECK(octet_length(sha256)=32), expires_at timestamptz NOT NULL, completed_at timestamptz)",
    "CREATE INDEX media_hash_idx ON media(sha256) WHERE deleted_at IS NULL AND verified",
    "ALTER TABLE outbox ADD COLUMN available_at timestamptz NOT NULL DEFAULT now(), ADD COLUMN last_error text",
    "CREATE FUNCTION sloray_content_purge() RETURNS trigger LANGUAGE plpgsql AS $$
        DECLARE paths jsonb;
        BEGIN
            IF TG_TABLE_NAME='posts' THEN
                paths := jsonb_build_array('/','/rss.xml','/sitemap.xml','/posts/'||NEW.slug);
                IF TG_OP='UPDATE' AND OLD.slug<>NEW.slug THEN paths:=paths||jsonb_build_array('/posts/'||OLD.slug); END IF;
            ELSE
                SELECT jsonb_build_array('/posts/'||slug) INTO paths FROM posts WHERE id=NEW.post_id;
            END IF;
            INSERT INTO outbox(id,topic,payload) VALUES(gen_random_uuid(),'cdn.purge',jsonb_build_object('paths',paths));
            RETURN NEW;
        END $$",
    "CREATE TRIGGER posts_purge_trigger AFTER INSERT OR UPDATE OF slug,title,summary,body_md,status,deleted_at,cover_media_id,published_at ON posts FOR EACH ROW EXECUTE FUNCTION sloray_content_purge()",
    "CREATE TRIGGER comments_purge_trigger AFTER UPDATE OF status ON comments FOR EACH ROW EXECUTE FUNCTION sloray_content_purge()",
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
            "DROP TRIGGER comments_purge_trigger ON comments",
            "DROP TRIGGER posts_purge_trigger ON posts",
            "DROP FUNCTION sloray_content_purge()",
            "ALTER TABLE outbox DROP COLUMN last_error,DROP COLUMN available_at",
            "DROP INDEX media_hash_idx",
            "DROP TABLE media_uploads",
            "ALTER TABLE media DROP COLUMN verified,DROP COLUMN deleted_at,DROP COLUMN variants_generated_at",
        ] {
            manager.get_connection().execute_unprepared(sql).await?;
        }
        Ok(())
    }
}
