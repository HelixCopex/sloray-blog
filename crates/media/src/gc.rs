use crate::{Error, Result, query, storage::Storage};
use sea_orm::{DatabaseConnection, FromQueryResult};
use sloray_config::Config;

/// 默认为 dry-run。只扫描本站 private/ 原图，保留 24h 和活跃上传意图。
pub async fn collect(db: &DatabaseConnection, config: &Config, apply: bool) -> Result<Vec<String>> {
    let storage = Storage::new(config)?;
    let mut continuation = None;
    let mut candidates = vec![];
    loop {
        let result = storage
            .client
            .list_objects_v2()
            .bucket(&storage.bucket)
            .prefix("private/")
            .set_continuation_token(continuation)
            .send()
            .await
            .map_err(anyhow::Error::new)?;
        for object in result.contents() {
            let Some(key) = object.key() else { continue };
            let Some(modified) = object.last_modified() else {
                continue;
            };
            if modified.secs() > chrono::Utc::now().timestamp() - 86400 {
                continue;
            }
            #[derive(FromQueryResult)]
            struct Exists {
                present: bool,
            }
            let exists:Exists=query::one(db,"SELECT EXISTS(SELECT 1 FROM media WHERE object_key=$1 UNION ALL SELECT 1 FROM media_uploads WHERE object_key=$1 AND expires_at>now()) AS present",vec![key.to_owned().into()]).await?.ok_or(Error::NotFound)?;
            if !exists.present {
                if apply {
                    storage.remove(key).await?;
                }
                candidates.push(key.to_owned());
            }
        }
        if !result.is_truncated().unwrap_or(false) {
            break;
        }
        continuation = result.next_continuation_token().map(str::to_owned);
        if continuation.is_none() {
            return Err(Error::Validation("对象列举缺少继续游标".into()));
        }
    }
    if apply {
        query::exec(db,"DELETE FROM media_uploads WHERE completed_at IS NULL AND expires_at<now()-interval '24 hours'",vec![]).await?;
    }
    Ok(candidates)
}
