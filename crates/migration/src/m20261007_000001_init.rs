//! 初始 schema：用户、会话、文章、修订、标签、评论、媒体、计数、点赞、outbox。
//!
//! # 为什么这里用原生 Postgres SQL，而不是 sea-query 的 DSL？
//!
//! 1. 本项目在 D5 里已经决定 **Postgres 是唯一权威源**，不存在多后端需求，
//!    DSL 的主要卖点（跨库可移植）在这里没有价值。
//! 2. sea-query 1.0 是刚发布的大版本，组合主键、`IdenList` 约束等 API 都有变动；
//!    用 DSL 生成的 DDL 在没跑过真实 Postgres 之前是**不可验证**的。
//!    原生 SQL 是任何 Postgres 从业者一眼就能审阅的，schema 变更的 review 成本最低。
//! 3. 可以用上 PG 原生能力：部分索引（`WHERE deleted_at IS NULL`）、`CHECK` 约束、
//!    `bytea`、复合主键，不必和 DSL 的类型系统较劲。
//!
//! **每个元素是一条独立语句**：sqlx 的扩展协议不接受一次多条语句，
//! 所以这里的粒度就是"一条 `execute_unprepared` = 一条语句"。

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// 建表语句，按依赖顺序排列（被引用的表在前）。
///
/// ID 一律 `uuid` 且**由应用层生成 UUIDv7**，不使用数据库自增序列：
/// 这样将来多写、导入或联邦时不会撞号，同时保留按时间排序的能力。
const UP: &[&str] = &[
    // ---------- 用户 ----------
    "CREATE TABLE users (
        id            uuid        PRIMARY KEY,
        email         text        NOT NULL UNIQUE,
        password_hash text        NOT NULL,
        display_name  text        NOT NULL,
        role          text        NOT NULL,
        created_at    timestamptz NOT NULL DEFAULT now(),
        updated_at    timestamptz NOT NULL DEFAULT now(),
        CONSTRAINT users_role_valid    CHECK (role IN ('admin', 'author')),
        CONSTRAINT users_email_folded  CHECK (email = lower(email))
    )",
    // ---------- 媒体（posts 会引用它，所以必须先建） ----------
    "CREATE TABLE media (
        id          uuid        PRIMARY KEY,
        object_key  text        NOT NULL UNIQUE,
        mime        text        NOT NULL,
        size_bytes  bigint      NOT NULL,
        width       integer,
        height      integer,
        sha256      bytea       NOT NULL,
        uploaded_by uuid        REFERENCES users (id) ON DELETE SET NULL,
        created_at  timestamptz NOT NULL DEFAULT now(),
        CONSTRAINT media_size_non_negative CHECK (size_bytes >= 0),
        CONSTRAINT media_sha256_is_32_bytes CHECK (octet_length(sha256) = 32)
    )",
    "CREATE INDEX media_uploaded_by_idx ON media (uploaded_by)",
    // ---------- 会话 ----------
    // 存 token 的哈希而不是明文：数据库泄漏不等于会话被接管。
    // 放在数据库而不是进程内存里，是为了让任意实例都能校验/撤销（无粘性会话）。
    "CREATE TABLE sessions (
        id         uuid        PRIMARY KEY,
        token_hash bytea       NOT NULL UNIQUE,
        user_id    uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        expires_at timestamptz NOT NULL,
        ip_hash    bytea,
        user_agent text,
        created_at timestamptz NOT NULL DEFAULT now()
    )",
    "CREATE INDEX sessions_user_id_idx ON sessions (user_id)",
    "CREATE INDEX sessions_expires_at_idx ON sessions (expires_at)",
    // ---------- 文章 ----------
    // version 用于乐观锁：两个作者同时编辑时，后提交者会收到 409 而不是静默覆盖。
    "CREATE TABLE posts (
        id             uuid        PRIMARY KEY,
        slug           text        NOT NULL UNIQUE,
        title          text        NOT NULL,
        summary        text        NOT NULL DEFAULT '',
        body_md        text        NOT NULL,
        status         text        NOT NULL,
        published_at   timestamptz,
        author_id      uuid        NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
        cover_media_id uuid        REFERENCES media (id) ON DELETE SET NULL,
        lang           text        NOT NULL DEFAULT 'zh',
        version        integer     NOT NULL DEFAULT 1,
        created_at     timestamptz NOT NULL DEFAULT now(),
        updated_at     timestamptz NOT NULL DEFAULT now(),
        deleted_at     timestamptz,
        CONSTRAINT posts_status_valid CHECK (status IN ('draft', 'published', 'scheduled')),
        -- published_at 同时承担「发布时间」与「定时发布时间」，因此这两类状态必须有值。
        CONSTRAINT posts_published_requires_timestamp
            CHECK (status = 'draft' OR published_at IS NOT NULL),
        CONSTRAINT posts_version_positive CHECK (version > 0)
    )",
    // 列表页查询热路径：部分索引，软删除的文章不进索引。
    "CREATE INDEX posts_listing_idx ON posts (status, published_at DESC) WHERE deleted_at IS NULL",
    "CREATE INDEX posts_author_idx ON posts (author_id)",
    "CREATE INDEX posts_cover_media_idx ON posts (cover_media_id)",
    // ---------- 修订历史（多作者场景的必备项） ----------
    "CREATE TABLE post_revisions (
        id         uuid        PRIMARY KEY,
        post_id    uuid        NOT NULL REFERENCES posts (id) ON DELETE CASCADE,
        title      text        NOT NULL,
        body_md    text        NOT NULL,
        editor_id  uuid        REFERENCES users (id) ON DELETE SET NULL,
        created_at timestamptz NOT NULL DEFAULT now()
    )",
    "CREATE INDEX post_revisions_post_idx ON post_revisions (post_id, created_at DESC)",
    // ---------- 标签 ----------
    "CREATE TABLE tags (
        id   uuid PRIMARY KEY,
        slug text NOT NULL UNIQUE,
        name text NOT NULL
    )",
    "CREATE TABLE post_tags (
        post_id uuid NOT NULL REFERENCES posts (id) ON DELETE CASCADE,
        tag_id  uuid NOT NULL REFERENCES tags (id) ON DELETE CASCADE,
        PRIMARY KEY (post_id, tag_id)
    )",
    "CREATE INDEX post_tags_tag_idx ON post_tags (tag_id)",
    // ---------- 评论 ----------
    // 默认 pending：先审后发，不接受任何未经审核的输入直接展示。
    "CREATE TABLE comments (
        id               uuid        PRIMARY KEY,
        post_id          uuid        NOT NULL REFERENCES posts (id) ON DELETE CASCADE,
        parent_id        uuid        REFERENCES comments (id) ON DELETE CASCADE,
        user_id          uuid        REFERENCES users (id) ON DELETE SET NULL,
        guest_name       text,
        guest_email_hash bytea,
        body_md          text        NOT NULL,
        status           text        NOT NULL DEFAULT 'pending',
        ip_hash          bytea,
        created_at       timestamptz NOT NULL DEFAULT now(),
        CONSTRAINT comments_status_valid CHECK (status IN ('pending', 'approved', 'spam')),
        -- 要么是登录用户，要么留下访客昵称，不允许匿名无主。
        CONSTRAINT comments_author_present CHECK (user_id IS NOT NULL OR guest_name IS NOT NULL)
    )",
    "CREATE INDEX comments_thread_idx ON comments (post_id, status, created_at)",
    "CREATE INDEX comments_parent_idx ON comments (parent_id)",
    // ---------- 浏览去重 ----------
    // 复合主键 = 唯一约束：同一访客同一天对同一文章的重复浏览会被
    // ON CONFLICT DO NOTHING 吃掉，因此天然幂等、重试安全，且不需要行锁竞争。
    // 这是"用唯一约束换取正确性"的典型例子，也完全兼容将来多节点写入。
    "CREATE TABLE post_view_daily (
        post_id      uuid        NOT NULL REFERENCES posts (id) ON DELETE CASCADE,
        day          date        NOT NULL,
        visitor_hash bytea       NOT NULL,
        created_at   timestamptz NOT NULL DEFAULT now(),
        PRIMARY KEY (post_id, day, visitor_hash)
    )",
    "CREATE INDEX post_view_daily_post_day_idx ON post_view_daily (post_id, day)",
    // ---------- 聚合计数（读路径只读这一张表） ----------
    "CREATE TABLE post_stats (
        post_id    uuid        PRIMARY KEY REFERENCES posts (id) ON DELETE CASCADE,
        view_count bigint      NOT NULL DEFAULT 0,
        like_count bigint      NOT NULL DEFAULT 0,
        updated_at timestamptz NOT NULL DEFAULT now(),
        CONSTRAINT post_stats_non_negative CHECK (view_count >= 0 AND like_count >= 0)
    )",
    // ---------- 点赞 ----------
    "CREATE TABLE likes (
        user_id    uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        post_id    uuid        NOT NULL REFERENCES posts (id) ON DELETE CASCADE,
        created_at timestamptz NOT NULL DEFAULT now(),
        PRIMARY KEY (user_id, post_id)
    )",
    "CREATE INDEX likes_post_idx ON likes (post_id)",
    // ---------- outbox ----------
    // 现在用不到（只有 Postgres 一个存储）。一旦引入第二个存储
    // （例如 Meilisearch 索引），就改走这张表 + worker 投递，绝不做双写。
    "CREATE TABLE outbox (
        id           uuid        PRIMARY KEY,
        topic        text        NOT NULL,
        payload      jsonb       NOT NULL,
        attempts     integer     NOT NULL DEFAULT 0,
        created_at   timestamptz NOT NULL DEFAULT now(),
        processed_at timestamptz
    )",
    "CREATE INDEX outbox_pending_idx ON outbox (created_at) WHERE processed_at IS NULL",
];

/// 回滚：逆依赖顺序删表。整体在一个事务里，失败即回滚。
const DOWN: &[&str] = &[
    "DROP TABLE IF EXISTS outbox",
    "DROP TABLE IF EXISTS likes",
    "DROP TABLE IF EXISTS post_stats",
    "DROP TABLE IF EXISTS post_view_daily",
    "DROP TABLE IF EXISTS comments",
    "DROP TABLE IF EXISTS post_tags",
    "DROP TABLE IF EXISTS tags",
    "DROP TABLE IF EXISTS post_revisions",
    "DROP TABLE IF EXISTS posts",
    "DROP TABLE IF EXISTS sessions",
    "DROP TABLE IF EXISTS media",
    "DROP TABLE IF EXISTS users",
];

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for statement in UP {
            manager
                .get_connection()
                .execute_unprepared(statement)
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for statement in DOWN {
            manager
                .get_connection()
                .execute_unprepared(statement)
                .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 逆向顺序必须是建立顺序的严格反序，否则删表会遇到外键依赖。
    #[test]
    fn down_is_exact_reverse_of_up() {
        let created: Vec<&str> = UP
            .iter()
            .filter_map(|s| {
                let s = s.trim_start();
                s.starts_with("CREATE TABLE")
                    .then(|| s.split_whitespace().nth(2).unwrap_or_default())
            })
            .collect();

        let dropped: Vec<&str> = DOWN
            .iter()
            .filter_map(|s| s.strip_prefix("DROP TABLE IF EXISTS "))
            .collect();

        let mut expected = created.clone();
        expected.reverse();

        assert_eq!(dropped, expected, "DOWN 必须是 UP 中建表顺序的严格反序");
    }

    #[test]
    fn every_create_table_has_a_primary_key() {
        // 没有主键的表将来无法安全地做逻辑复制（只读副本/逻辑订阅要求主键）。
        for statement in UP
            .iter()
            .filter(|s| s.trim_start().starts_with("CREATE TABLE"))
        {
            assert!(
                statement.contains("PRIMARY KEY"),
                "建表语句缺少主键，会阻碍将来的逻辑复制：\n{statement}"
            );
        }
    }

    #[test]
    fn statements_are_single_line_free_and_terminator_free() {
        // 每条语句独立执行，因此不能带结尾分号，也不能包含多条语句。
        for statement in UP.iter().chain(DOWN.iter()) {
            assert!(
                !statement.trim_end().ends_with(';'),
                "语句不应以分号结尾（会被逐条执行）：\n{statement}"
            );
        }
    }

    #[test]
    fn derived_migration_name_comes_from_file_stem() {
        // DeriveMigrationName 用 file!() 的文件名做迁移名，
        // 所以文件名就是记录在 seaql_migrations 里的标识，改名等于改历史。
        assert_eq!(Migration.name(), "m20261007_000001_init");
    }
}
