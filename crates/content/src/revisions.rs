use crate::{
    Actor, Error, Result,
    dto::{Revision, RevisionPage},
    posts, query,
};
use sea_orm::DatabaseConnection;
use uuid::Uuid;

pub async fn list(
    db: &DatabaseConnection,
    actor: Actor,
    id: Uuid,
    cursor: Option<&str>,
    limit: u32,
) -> Result<RevisionPage> {
    posts::owned(db, id, actor).await?;
    let scope = format!("revisions:{id}");
    let cursor = crate::pagination::decode(cursor, &scope)?;
    let limit = limit.clamp(1, 50) as usize;
    let mut items:Vec<Revision>=query::all(db, "SELECT id,post_id,title,body_md,editor_id,created_at FROM post_revisions WHERE post_id=$1 AND ($2::timestamptz IS NULL OR (created_at,id)<($2,$3::uuid)) ORDER BY created_at DESC,id DESC LIMIT $4", vec![id.into(),cursor.as_ref().map(|c|c.time).into(),cursor.as_ref().map(|c|c.id).into(),((limit+1) as i64).into()]).await?;
    let next_cursor = if items.len() > limit {
        items.truncate(limit);
        let last = items.last().ok_or(Error::NotFound)?;
        Some(crate::pagination::encode(last.created_at, last.id, scope)?)
    } else {
        None
    };
    Ok(RevisionPage { items, next_cursor })
}
