//! 从权威原文生成派生文本；行锁保证手工修复与重建不互相覆盖。
use crate::{Error, Result, query};
use jieba_rs::Jieba;
use sea_orm::{ConnectionTrait, DatabaseConnection, FromQueryResult, TransactionTrait};
use std::sync::OnceLock;
use uuid::Uuid;

static JIEBA: OnceLock<Jieba> = OnceLock::new();

pub fn segment(text: &str, search: bool) -> String {
    let jieba = JIEBA.get_or_init(Jieba::new);
    let tokens = if search {
        jieba.cut_for_search(text, true)
    } else {
        jieba.cut(text, true)
    };
    tokens
        .iter()
        .map(|t| t.word)
        .filter(|s| s.chars().any(char::is_alphanumeric))
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(FromQueryResult)]
pub(crate) struct Settings {
    pub engine: String,
    pub config: String,
}

pub(crate) async fn settings<C: ConnectionTrait>(db: &C) -> Result<Settings> {
    query::one(db, "SELECT engine,config FROM search_settings", vec![])
        .await?
        .ok_or(Error::NotFound)
}

pub async fn index<C: ConnectionTrait>(db: &C, id: Uuid) -> Result<()> {
    if settings(db).await?.engine == "database" {
        return Ok(());
    }
    #[derive(FromQueryResult)]
    struct Source {
        title: String,
        summary: String,
        body_md: String,
    }
    let source: Source = query::one(
        db,
        "SELECT title,summary,body_md FROM posts WHERE id=$1 FOR UPDATE",
        vec![id.into()],
    )
    .await?
    .ok_or(Error::NotFound)?;
    query::exec(db,"UPDATE posts SET search_title=$1,search_summary=$2,search_body=$3,search_source=md5(jsonb_build_array(title,summary,body_md)::text) WHERE id=$4",vec![segment(&source.title,true).into(),segment(&source.summary,true).into(),segment(&source.body_md,true).into(),id.into()]).await?;
    Ok(())
}

pub async fn pending(db: &DatabaseConnection) -> Result<u64> {
    let txn = db.begin().await?;
    #[derive(FromQueryResult)]
    struct Id {
        id: Uuid,
    }
    let ids:Vec<Id>=query::all(&txn,"SELECT id FROM posts WHERE search_vector IS NULL AND deleted_at IS NULL ORDER BY id LIMIT 50 FOR UPDATE SKIP LOCKED",vec![]).await?;
    for id in &ids {
        index(&txn, id.id).await?;
    }
    txn.commit().await?;
    Ok(ids.len() as u64)
}

pub async fn rebuild(db: &DatabaseConnection) -> Result<u64> {
    if settings(db).await?.engine == "database" {
        #[derive(FromQueryResult)]
        struct Count {
            count: i64,
        }
        let result: Count = query::one(db, "SELECT rebuild_posts_search_vector() AS count", vec![])
            .await?
            .ok_or(Error::NotFound)?;
        return Ok(result.count as u64);
    }
    query::exec(
        db,
        "UPDATE posts SET search_vector=NULL WHERE deleted_at IS NULL",
        vec![],
    )
    .await?;
    let mut total = 0;
    loop {
        let count = pending(db).await?;
        total += count;
        if count == 0 {
            return Ok(total);
        }
    }
}
