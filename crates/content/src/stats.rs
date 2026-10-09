use crate::{Error, Result, activity_dto::Stats, query};
use sea_orm::{ConnectionTrait, DatabaseConnection, TransactionTrait};
use uuid::Uuid;

async fn visible<C: ConnectionTrait>(db: &C, id: Uuid) -> Result<()> {
    #[derive(sea_orm::FromQueryResult)]
    struct Exists {
        present: bool,
    }
    let row = query::one::<Exists,_>(db,"SELECT EXISTS(SELECT 1 FROM posts WHERE id=$1 AND status='published' AND deleted_at IS NULL AND published_at<=now()) AS present",vec![id.into()]).await?.ok_or(Error::NotFound)?;
    if !row.present {
        return Err(Error::NotFound);
    }
    Ok(())
}

pub async fn get(db: &DatabaseConnection, id: Uuid, user: Option<Uuid>) -> Result<Stats> {
    visible(db, id).await?;
    query::one(db,"SELECT coalesce(s.view_count,0)::bigint AS view_count,coalesce(s.like_count,0)::bigint AS like_count,EXISTS(SELECT 1 FROM likes WHERE post_id=$1 AND user_id=$2) AS liked FROM posts p LEFT JOIN post_stats s ON s.post_id=p.id WHERE p.id=$1",vec![id.into(),user.into()]).await?.ok_or(Error::NotFound)
}

pub async fn view(db: &DatabaseConnection, id: Uuid, visitor_hash: Vec<u8>) -> Result<()> {
    let txn = db.begin().await?;
    visible(&txn, id).await?;
    let added = query::exec(&txn,"INSERT INTO post_view_daily(post_id,day,visitor_hash) VALUES($1,(now() AT TIME ZONE 'UTC')::date,$2) ON CONFLICT DO NOTHING",vec![id.into(),visitor_hash.into()]).await?;
    if added > 0 {
        query::exec(&txn,"INSERT INTO post_stats(post_id,view_count) VALUES($1,1) ON CONFLICT(post_id) DO UPDATE SET view_count=post_stats.view_count+1,updated_at=now()",vec![id.into()]).await?;
    }
    txn.commit().await?;
    Ok(())
}

pub async fn like(db: &DatabaseConnection, id: Uuid, user: Uuid, liked: bool) -> Result<()> {
    let txn = db.begin().await?;
    visible(&txn, id).await?;
    // 同一篇文章的 toggle 与重建共享事务咨询锁，避免并发取消/点赞计数失序。
    query::exec(
        &txn,
        "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
        vec![id.to_string().into()],
    )
    .await?;
    let changed = if liked {
        query::exec(
            &txn,
            "INSERT INTO likes(user_id,post_id) VALUES($1,$2) ON CONFLICT DO NOTHING",
            vec![user.into(), id.into()],
        )
        .await?
    } else {
        query::exec(
            &txn,
            "DELETE FROM likes WHERE user_id=$1 AND post_id=$2",
            vec![user.into(), id.into()],
        )
        .await?
    };
    if changed > 0 {
        let delta: i64 = if liked { 1 } else { -1 };
        query::exec(&txn,"INSERT INTO post_stats(post_id,like_count) VALUES($1,greatest($2,0)) ON CONFLICT(post_id) DO UPDATE SET like_count=greatest(post_stats.like_count+$2,0),updated_at=now()",vec![id.into(),delta.into()]).await?;
    }
    txn.commit().await?;
    Ok(())
}

pub async fn rebuild(db: &DatabaseConnection) -> Result<u64> {
    let txn = db.begin().await?;
    // 与浏览/点赞的插入事务串行化；避免读取旧快照后覆盖新计数。
    query::exec(
        &txn,
        "LOCK TABLE post_view_daily,likes,post_stats IN SHARE ROW EXCLUSIVE MODE",
        vec![],
    )
    .await?;
    let count = query::exec(&txn,"INSERT INTO post_stats(post_id,view_count,like_count) SELECT p.id,(SELECT count(*) FROM post_view_daily v WHERE v.post_id=p.id),(SELECT count(*) FROM likes l WHERE l.post_id=p.id) FROM posts p ON CONFLICT(post_id) DO UPDATE SET view_count=excluded.view_count,like_count=excluded.like_count,updated_at=now()",vec![]).await?;
    txn.commit().await?;
    Ok(count)
}
