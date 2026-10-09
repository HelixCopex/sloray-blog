//! 评论默认 pending；授权和楼中楼约束在同一业务层。
use crate::{Actor, Error, Result, activity_dto::*, pagination, posts, query};
use sea_orm::{DatabaseConnection, TransactionTrait};
use uuid::Uuid;

const FIELDS: &str = "c.id,c.post_id,c.parent_id,coalesce(u.display_name,c.guest_name,'访客') AS author_name,c.body_md,c.status,c.created_at";

pub async fn list(
    db: &DatabaseConnection,
    slug: &str,
    cursor: Option<&str>,
    limit: u32,
) -> Result<CommentPage> {
    let post = posts::by_slug(db, slug).await?;
    page(db, &format!("post:{}",post.id), cursor, limit,
        "c.post_id=$1 AND c.status='approved' AND NOT EXISTS(WITH RECURSIVE ancestors AS (SELECT id,parent_id,status FROM comments WHERE id=c.parent_id UNION ALL SELECT p.id,p.parent_id,p.status FROM comments p JOIN ancestors a ON p.id=a.parent_id) SELECT 1 FROM ancestors WHERE status<>'approved')", vec![post.id.into()]).await
}

pub async fn queue(
    db: &DatabaseConnection,
    actor: Actor,
    status: &str,
    cursor: Option<&str>,
    limit: u32,
) -> Result<CommentPage> {
    if !["pending", "approved", "spam"].contains(&status) {
        return Err(Error::Validation("评论状态无效".into()));
    }
    page(
        db,
        &format!("queue:{}:{}:{status}", actor.id, actor.admin),
        cursor,
        limit,
        "c.status=$1 AND ($2 OR p.author_id=$3) AND p.deleted_at IS NULL",
        vec![
            status.to_owned().into(),
            actor.admin.into(),
            actor.id.into(),
        ],
    )
    .await
}

async fn page(
    db: &DatabaseConnection,
    scope: &str,
    cursor: Option<&str>,
    limit: u32,
    filter: &str,
    mut values: Vec<sea_orm::Value>,
) -> Result<CommentPage> {
    let cursor = pagination::decode(cursor, scope)?;
    let limit = limit.clamp(1, 50) as usize;
    let n = values.len() + 1;
    values.extend([
        cursor.as_ref().map(|c| c.time).into(),
        cursor.as_ref().map(|c| c.id).into(),
        ((limit + 1) as i64).into(),
    ]);
    let mut items: Vec<Comment> = query::all(db, &format!("SELECT {FIELDS} FROM comments c JOIN posts p ON p.id=c.post_id LEFT JOIN users u ON u.id=c.user_id WHERE {filter} AND (${n}::timestamptz IS NULL OR (c.created_at,c.id) < (${n},${}::uuid)) ORDER BY c.created_at DESC,c.id DESC LIMIT ${}",n+1,n+2),values).await?;
    let next_cursor = if items.len() > limit {
        items.truncate(limit);
        let last = items.last().ok_or(Error::NotFound)?;
        Some(pagination::encode(
            last.created_at,
            last.id,
            scope.to_owned(),
        )?)
    } else {
        None
    };
    Ok(CommentPage { items, next_cursor })
}

/// 蜜罐请求不入库，返回 None；外部统一返回 accepted。
pub async fn create(
    db: &DatabaseConnection,
    slug: &str,
    user: Option<Uuid>,
    ip_hash: Vec<u8>,
    input: WriteComment,
) -> Result<()> {
    if !input.website.is_empty() {
        return Ok(());
    }
    if input.body_md.trim().is_empty()
        || input.body_md.chars().count() > 4000
        || input.guest_name.trim().is_empty()
        || input.guest_name.chars().count() > 80
    {
        return Err(Error::Validation(
            "评论正文限 1–4000 字，昵称限 1–80 字".into(),
        ));
    }
    let txn = db.begin().await?;
    #[derive(sea_orm::FromQueryResult)]
    struct PostId {
        id: Uuid,
    }
    let post = query::one::<PostId, _>(&txn, "SELECT id FROM posts WHERE slug=$1 AND status='published' AND published_at<=now() AND deleted_at IS NULL FOR SHARE", vec![slug.to_owned().into()]).await?.ok_or(Error::NotFound)?;
    if let Some(parent) = input.parent_id {
        #[derive(sea_orm::FromQueryResult)]
        struct Parent {
            depth: i64,
            valid: bool,
        }
        let parent = query::one::<Parent,_>(&txn,
            "WITH RECURSIVE ancestors AS (SELECT id,parent_id,post_id,status FROM comments WHERE id=$1 UNION ALL SELECT c.id,c.parent_id,c.post_id,c.status FROM comments c JOIN ancestors a ON c.id=a.parent_id) SELECT count(*)::bigint AS depth,coalesce(bool_and(post_id=$2 AND status='approved'),false) AS valid FROM ancestors",
            vec![parent.into(),post.id.into()]).await?.ok_or(Error::NotFound)?;
        if !parent.valid || parent.depth >= 5 {
            return Err(Error::Validation("回复对象不可用或层级过深".into()));
        }
    }
    query::exec(&txn,"INSERT INTO comments(id,post_id,parent_id,user_id,guest_name,body_md,ip_hash) VALUES($1,$2,$3,$4,$5,$6,$7)", vec![Uuid::now_v7().into(),post.id.into(),input.parent_id.into(),user.into(),input.guest_name.trim().to_owned().into(),input.body_md.trim().to_owned().into(),ip_hash.into()]).await?;
    txn.commit().await?;
    Ok(())
}

pub async fn moderate(
    db: &DatabaseConnection,
    actor: Actor,
    id: Uuid,
    input: ModerateComment,
) -> Result<()> {
    if !["pending", "approved", "spam"].contains(&input.status.as_str()) {
        return Err(Error::Validation("评论状态无效".into()));
    }
    let affected = query::exec(db,"UPDATE comments c SET status=$1 FROM posts p WHERE c.id=$2 AND p.id=c.post_id AND p.deleted_at IS NULL AND ($3 OR p.author_id=$4)",vec![input.status.into(),id.into(),actor.admin.into(),actor.id.into()]).await?;
    if affected == 0 {
        return Err(Error::NotFound);
    }
    Ok(())
}
