//! 无进程内任务状态；多个 worker 用行锁分批发布到期文章。
use crate::{Result, query};
use sea_orm::{DatabaseConnection, TransactionTrait};

pub async fn tick(db: &DatabaseConnection) -> Result<u64> {
    crate::search_index::pending(db).await?;
    let txn = db.begin().await?;
    let published = query::exec(&txn,"WITH due AS (SELECT id FROM posts WHERE status='scheduled' AND published_at<=now() AND deleted_at IS NULL ORDER BY published_at,id LIMIT 100 FOR UPDATE SKIP LOCKED) UPDATE posts p SET status='published',updated_at=now(),version=version+1 FROM due WHERE p.id=due.id",vec![]).await?;
    query::exec(
        &txn,
        "DELETE FROM request_limits WHERE expires_at<now()",
        vec![],
    )
    .await?;
    query::exec(&txn, "DELETE FROM sessions WHERE expires_at<now()", vec![]).await?;
    query::exec(
        &txn,
        "DELETE FROM outbox WHERE processed_at<now()-interval '30 days'",
        vec![],
    )
    .await?;
    txn.commit().await?;
    Ok(published)
}
