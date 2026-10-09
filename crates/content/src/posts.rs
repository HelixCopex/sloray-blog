//! 文章读写、授权、游标和修订在业务层保持一致。
use crate::{Actor, Error, Result, dto::*, query};
use chrono::{DateTime, Utc};
use sea_orm::{ConnectionTrait, DatabaseConnection, TransactionTrait};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const SUMMARY: &str = "p.id,p.slug,p.title,p.summary,p.published_at,p.author_id,u.display_name AS author_name,p.version,p.updated_at,p.status,coalesce(s.view_count,0)::bigint AS view_count,coalesce(s.like_count,0)::bigint AS like_count";
const DETAIL: &str = "p.id,p.slug,p.title,p.summary,p.body_md,p.status,p.published_at,p.author_id,u.display_name AS author_name,p.cover_media_id,p.lang,p.version,p.created_at,p.updated_at";
const PUBLIC: &str = "p.status='published' AND p.deleted_at IS NULL AND p.published_at <= now()";

#[derive(Serialize, Deserialize)]
struct Cursor {
    time: DateTime<Utc>,
    id: Uuid,
    tag: Option<String>,
}

pub fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 160
        && slug
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

fn validate(input: &WritePost) -> Result<()> {
    if !valid_slug(&input.slug)
        || input.title.trim().is_empty()
        || input.title.chars().count() > 200
    {
        return Err(Error::Validation("slug 或标题不合法".into()));
    }
    if input.body_md.len() > 512_000 || input.summary.chars().count() > 1000 {
        return Err(Error::Validation("正文或摘要过长".into()));
    }
    if !["draft", "published", "scheduled"].contains(&input.status.as_str()) {
        return Err(Error::Validation("文章状态不合法".into()));
    }
    if input.status == "scheduled" && input.published_at.is_none_or(|date| date <= Utc::now()) {
        return Err(Error::Validation("定时发布时间必须在未来".into()));
    }
    if input.status == "published" && input.published_at.is_some_and(|date| date > Utc::now()) {
        return Err(Error::Validation(
            "未来发布时间应使用 scheduled 状态".into(),
        ));
    }
    if input.tags.len() > 20
        || input.tags.iter().any(|tag| !valid_slug(tag))
        || input.lang.len() > 20
    {
        return Err(Error::Validation("标签或语言不合法".into()));
    }
    Ok(())
}

pub async fn list(
    db: &DatabaseConnection,
    cursor: Option<&str>,
    tag: Option<&str>,
    limit: u32,
) -> Result<PostPage> {
    let limit = limit.clamp(1, 50);
    let cursor = cursor
        .map(|value| {
            if value.len() > 1000 {
                return Err(Error::Validation("游标过长".into()));
            }
            let bytes = hex::decode(value).map_err(|_| Error::Validation("游标无效".into()))?;
            serde_json::from_slice::<Cursor>(&bytes)
                .map_err(|_| Error::Validation("游标无效".into()))
        })
        .transpose()?;
    if cursor
        .as_ref()
        .is_some_and(|value| value.tag.as_deref() != tag)
    {
        return Err(Error::Validation("游标与标签筛选不匹配".into()));
    }
    let sql = format!(
        "SELECT {SUMMARY} FROM posts p JOIN users u ON u.id=p.author_id LEFT JOIN post_stats s ON s.post_id=p.id WHERE {PUBLIC} AND ($1::text IS NULL OR EXISTS(SELECT 1 FROM post_tags pt JOIN tags t ON t.id=pt.tag_id WHERE pt.post_id=p.id AND t.slug=$1)) AND ($2::timestamptz IS NULL OR (p.published_at,p.id) < ($2,$3::uuid)) ORDER BY p.published_at DESC,p.id DESC LIMIT $4"
    );
    let mut items: Vec<PostSummary> = query::all(
        db,
        &sql,
        vec![
            tag.map(str::to_owned).into(),
            cursor.as_ref().map(|c| c.time).into(),
            cursor.as_ref().map(|c| c.id).into(),
            (i64::from(limit) + 1).into(),
        ],
    )
    .await?;
    let next_cursor = if items.len() > limit as usize {
        items.truncate(limit as usize);
        let last = items.last().ok_or(Error::NotFound)?;
        Some(hex::encode(
            serde_json::to_vec(&Cursor {
                time: last.published_at.ok_or(Error::NotFound)?,
                id: last.id,
                tag: tag.map(str::to_owned),
            })
            .map_err(|e| Error::Validation(e.to_string()))?,
        ))
    } else {
        None
    };
    Ok(PostPage { items, next_cursor })
}

pub async fn by_slug(db: &DatabaseConnection, slug: &str) -> Result<Post> {
    let sql = format!(
        "SELECT {DETAIL} FROM posts p JOIN users u ON u.id=p.author_id WHERE p.slug=$1 AND {PUBLIC}"
    );
    let mut post = query::one::<Post, _>(db, &sql, vec![slug.to_owned().into()])
        .await?
        .ok_or(Error::NotFound)?;
    post.tags = post_tags(db, post.id).await?;
    Ok(post)
}

pub async fn owned<C: ConnectionTrait>(db: &C, id: Uuid, actor: Actor) -> Result<Post> {
    let sql = format!(
        "SELECT {DETAIL} FROM posts p JOIN users u ON u.id=p.author_id WHERE p.id=$1 AND p.deleted_at IS NULL"
    );
    let mut post = query::one::<Post, _>(db, &sql, vec![id.into()])
        .await?
        .ok_or(Error::NotFound)?;
    if !actor.admin && post.author_id != actor.id {
        return Err(Error::Forbidden);
    }
    post.tags = post_tags(db, post.id).await?;
    Ok(post)
}

pub async fn tags(db: &DatabaseConnection) -> Result<Vec<Tag>> {
    query::all(db, &format!("SELECT DISTINCT t.id,t.slug,t.name FROM tags t JOIN post_tags pt ON pt.tag_id=t.id JOIN posts p ON p.id=pt.post_id WHERE {PUBLIC} ORDER BY t.slug"), vec![]).await
}

async fn post_tags<C: ConnectionTrait>(db: &C, id: Uuid) -> Result<Vec<Tag>> {
    query::all(db, "SELECT t.id,t.slug,t.name FROM tags t JOIN post_tags pt ON pt.tag_id=t.id WHERE pt.post_id=$1 ORDER BY t.slug", vec![id.into()]).await
}

pub async fn admin_list(
    db: &DatabaseConnection,
    actor: Actor,
    cursor: Option<&str>,
    limit: u32,
) -> Result<PostPage> {
    let scope = format!("admin:{}:{}", actor.id, actor.admin);
    let cursor = crate::pagination::decode(cursor, &scope)?;
    let limit = limit.clamp(1, 50) as usize;
    let mut items:Vec<PostSummary>=query::all(db, &format!("SELECT {SUMMARY} FROM posts p JOIN users u ON u.id=p.author_id LEFT JOIN post_stats s ON s.post_id=p.id WHERE p.deleted_at IS NULL AND ($1 OR p.author_id=$2) AND ($3::timestamptz IS NULL OR (p.updated_at,p.id)<($3,$4::uuid)) ORDER BY p.updated_at DESC,p.id DESC LIMIT $5"), vec![actor.admin.into(), actor.id.into(),cursor.as_ref().map(|c|c.time).into(),cursor.as_ref().map(|c|c.id).into(),((limit+1) as i64).into()]).await?;
    let next_cursor = if items.len() > limit {
        items.truncate(limit);
        let last = items.last().ok_or(Error::NotFound)?;
        Some(crate::pagination::encode(last.updated_at, last.id, scope)?)
    } else {
        None
    };
    Ok(PostPage { items, next_cursor })
}

pub async fn save(
    db: &DatabaseConnection,
    actor: Actor,
    id: Option<Uuid>,
    input: WritePost,
) -> Result<Post> {
    validate(&input)?;
    let txn = db.begin().await?;
    if let Some(cover) = input.cover_media_id {
        #[derive(sea_orm::FromQueryResult)]
        struct Cover {
            id: Uuid,
        }
        let cover=query::one::<Cover,_>(&txn,"SELECT id FROM media WHERE id=$1 AND deleted_at IS NULL AND verified AND variants_generated_at IS NOT NULL FOR SHARE",vec![cover.into()]).await?;
        if cover.is_none() {
            return Err(Error::Validation("封面图片尚未就绪或已删除".into()));
        }
        let _ = cover.map(|cover| cover.id);
    }
    let id = if let Some(id) = id {
        owned(&txn, id, actor).await?;
        let expected = input
            .version
            .ok_or_else(|| Error::Validation("更新必须提供 version".into()))?;
        let sql = "UPDATE posts SET slug=$1,title=$2,summary=$3,body_md=$4,status=$5,published_at=$6,cover_media_id=$7,lang=$8,version=version+1,updated_at=now() WHERE id=$9 AND version=$10 AND deleted_at IS NULL";
        if query::exec(
            &txn,
            sql,
            vec![
                input.slug.clone().into(),
                input.title.clone().into(),
                input.summary.clone().into(),
                input.body_md.clone().into(),
                input.status.clone().into(),
                input
                    .published_at
                    .or_else(|| (input.status == "published").then(Utc::now))
                    .into(),
                input.cover_media_id.into(),
                input.lang.clone().into(),
                id.into(),
                expected.into(),
            ],
        )
        .await?
            != 1
        {
            return Err(Error::Conflict);
        }
        id
    } else {
        let id = Uuid::now_v7();
        query::exec(&txn, "INSERT INTO posts(id,slug,title,summary,body_md,status,published_at,author_id,cover_media_id,lang) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)", vec![id.into(), input.slug.clone().into(), input.title.clone().into(), input.summary.clone().into(), input.body_md.clone().into(), input.status.clone().into(), input.published_at.or_else(|| (input.status == "published").then(Utc::now)).into(), actor.id.into(), input.cover_media_id.into(), input.lang.into()]).await?;
        id
    };
    query::exec(
        &txn,
        "INSERT INTO post_revisions(id,post_id,title,body_md,editor_id) VALUES($1,$2,$3,$4,$5)",
        vec![
            Uuid::now_v7().into(),
            id.into(),
            input.title.into(),
            input.body_md.into(),
            actor.id.into(),
        ],
    )
    .await?;
    query::exec(
        &txn,
        "DELETE FROM post_tags WHERE post_id=$1",
        vec![id.into()],
    )
    .await?;
    for tag in input.tags {
        query::exec(
            &txn,
            "INSERT INTO tags(id,slug,name) VALUES($1,$2,$2) ON CONFLICT(slug) DO NOTHING",
            vec![Uuid::now_v7().into(), tag.clone().into()],
        )
        .await?;
        query::exec(&txn, "INSERT INTO post_tags(post_id,tag_id) SELECT $1,id FROM tags WHERE slug=$2 ON CONFLICT DO NOTHING", vec![id.into(), tag.into()]).await?;
    }
    crate::search_index::index(&txn, id).await?;
    let post = owned(&txn, id, actor).await?;
    txn.commit().await?;
    Ok(post)
}

pub async fn delete(db: &DatabaseConnection, actor: Actor, id: Uuid, version: i32) -> Result<()> {
    let txn = db.begin().await?;
    owned(&txn, id, actor).await?;
    if query::exec(&txn, "UPDATE posts SET deleted_at=now(),updated_at=now(),version=version+1 WHERE id=$1 AND version=$2 AND deleted_at IS NULL", vec![id.into(), version.into()]).await? != 1 {
        return Err(Error::Conflict);
    }
    txn.commit().await?;
    Ok(())
}
