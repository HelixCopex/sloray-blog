//! outbox 行锁覆盖投递，失败保留任务并退避；原图字节不落盘。
use crate::{
    Error, Result, query,
    storage::{self, MAX_BYTES, Storage},
    uploads::Row,
};
use image::{GenericImageView, ImageFormat, ImageReader, Limits};
use sea_orm::{ConnectionTrait, DatabaseConnection, FromQueryResult, TransactionTrait};
use sha2::{Digest, Sha256};
use sloray_config::Config;
use uuid::Uuid;

#[derive(FromQueryResult)]
struct Job {
    id: Uuid,
    topic: String,
    payload: serde_json::Value,
    attempts: i32,
}

pub async fn tick(db: &DatabaseConnection, config: &Config) -> Result<u64> {
    let mut completed = 0;
    for _ in 0..20 {
        let txn = db.begin().await?;
        let Some(job)=query::one::<Job,_>(&txn,"SELECT id,topic,payload,attempts FROM outbox WHERE processed_at IS NULL AND available_at<=now() AND topic IN ('media.generate','media.delete','cdn.purge') ORDER BY created_at,id LIMIT 1 FOR UPDATE SKIP LOCKED",vec![]).await? else {break};
        let result = match job.topic.as_str() {
            "media.generate" => generate(&txn, config, &job.payload).await,
            "media.delete" => delete(config, &job.payload).await,
            "cdn.purge" => purge(config, &job.payload).await,
            _ => unreachable!("SQL restricts supported topics"),
        };
        match result {
            Ok(()) => {
                query::exec(
                    &txn,
                    "UPDATE outbox SET processed_at=now(),last_error=NULL WHERE id=$1",
                    vec![job.id.into()],
                )
                .await?;
                completed += 1;
            }
            Err(err) => {
                // 错误详情仅写日志，数据库状态不保存签名 URL / 凭据。
                tracing::warn!(job=%job.id,topic=%job.topic,kind=?std::mem::discriminant(&err),"outbox 投递失败（凭据与签名 URL 不写日志）");
                let delay = (1_i64 << (job.attempts.clamp(0, 9) + 1)).min(900);
                query::exec(&txn,"UPDATE outbox SET attempts=attempts+1,available_at=now()+$2::bigint*interval '1 second',last_error='delivery failed; inspect worker journal' WHERE id=$1",vec![job.id.into(),delay.into()]).await?;
            }
        }
        txn.commit().await?;
    }
    Ok(completed)
}

fn identifier(payload: &serde_json::Value) -> Result<Uuid> {
    payload
        .get("id")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| Error::Validation("任务媒体 ID 无效".into()))
}

pub async fn regenerate(db: &DatabaseConnection) -> Result<u64> {
    query::exec(db,"INSERT INTO outbox(id,topic,payload) SELECT gen_random_uuid(),'media.generate',jsonb_build_object('id',id) FROM media WHERE deleted_at IS NULL",vec![]).await
}

async fn generate<C: ConnectionTrait>(
    db: &C,
    config: &Config,
    payload: &serde_json::Value,
) -> Result<()> {
    let id = identifier(payload)?;
    // 生成任务与删除同步串行化；避免删除完成后另一个 worker 重新创建公开对象。
    let Some(row) = query::one::<Row, _>(
        db,
        "SELECT * FROM media WHERE id=$1 AND deleted_at IS NULL FOR UPDATE",
        vec![id.into()],
    )
    .await?
    else {
        return Ok(());
    };
    let storage = Storage::new(config)?;
    let bytes = storage
        .read(&row.object_key, MAX_BYTES as usize, false)
        .await?;
    if bytes.len() as i64 != row.size_bytes || Sha256::digest(&bytes).as_slice() != row.sha256 {
        return Err(Error::Validation("原图 SHA-256 或大小校验失败".into()));
    }
    let variants = tokio::task::spawn_blocking(move || -> Result<_> {
        let mut reader = ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .map_err(anyhow::Error::new)?;
        let mut limits = Limits::default();
        limits.max_image_width = Some(10_000);
        limits.max_image_height = Some(10_000);
        limits.max_alloc = Some(128 * 1024 * 1024);
        reader.limits(limits);
        let image = reader.decode().map_err(anyhow::Error::new)?;
        let (width, height) = image.dimensions();
        if u64::from(width) * u64::from(height) > 32_000_000 {
            return Err(Error::Validation("图片像素过大".into()));
        }
        let mut variants = vec![];
        // 小于 320px 的原图仍生成一个原尺寸 WebP，保证可公开展示。
        let widths = storage::WIDTHS
            .into_iter()
            .filter(|w| *w <= width as i32)
            .collect::<Vec<_>>();
        let widths = if widths.is_empty() {
            vec![width as i32]
        } else {
            widths
        };
        for size in widths {
            let resized =
                image.resize(size as u32, u32::MAX, image::imageops::FilterType::Lanczos3);
            let mut output = std::io::Cursor::new(Vec::new());
            resized
                .write_to(&mut output, ImageFormat::WebP)
                .map_err(anyhow::Error::new)?;
            variants.push((size, output.into_inner()));
        }
        Ok((width, height, variants))
    })
    .await
    .map_err(anyhow::Error::new)??;
    for (width, bytes) in variants.2 {
        storage
            .variant(
                &storage::variant_key(id, width),
                bytes,
                config.cloudflare_api_token.is_some(),
            )
            .await?;
    }
    query::exec(
        db,
        "UPDATE media SET verified=true,width=$2,height=$3,variants_generated_at=now() WHERE id=$1",
        vec![
            id.into(),
            (variants.0 as i32).into(),
            (variants.1 as i32).into(),
        ],
    )
    .await?;
    Ok(())
}

async fn delete(config: &Config, payload: &serde_json::Value) -> Result<()> {
    let id = identifier(payload)?;
    let key = payload
        .get("key")
        .and_then(|v| v.as_str())
        .filter(|s| s.starts_with("private/"))
        .ok_or_else(|| Error::Validation("原图路径无效".into()))?;
    let storage = Storage::new(config)?;
    storage.remove(key).await?;
    let mut urls = vec![];
    // 删除所有已生成尺寸（包括小图）由任务携带的宽度补充。
    let mut widths = storage::WIDTHS.to_vec();
    if let Some(width) = payload
        .get("width")
        .and_then(|v| v.as_i64())
        .filter(|w| *w > 0 && *w < 320)
    {
        widths.push(width as i32);
    }
    for width in widths {
        let key = storage::variant_key(id, width);
        storage.remove_variant(&key).await?;
        if let Some(url) = storage::url(config, &key) {
            urls.push(url);
        }
    }
    purge(config, &serde_json::json!({"urls":urls})).await
}

async fn purge(config: &Config, payload: &serde_json::Value) -> Result<()> {
    let (Some(zone), Some(token)) = (&config.cloudflare_zone_id, &config.cloudflare_api_token)
    else {
        return Ok(());
    };
    let mut urls = payload
        .get("urls")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if let Some(paths) = payload.get("paths").and_then(|v| v.as_array())
        && let Some(site) = &config.public_site_url
    {
        for path in paths.iter().filter_map(|v| v.as_str()) {
            let url = reqwest::Url::parse(site)
                .and_then(|base| base.join(path))
                .map_err(anyhow::Error::new)?;
            urls.push(serde_json::Value::String(url.to_string()));
        }
    }
    if urls.is_empty() {
        return Ok(());
    }
    let base = config
        .cloudflare_api_base_url
        .as_deref()
        .unwrap_or("https://api.cloudflare.com/client/v4");
    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(anyhow::Error::new)?
        .post(format!(
            "{}/zones/{zone}/purge_cache",
            base.trim_end_matches('/')
        ))
        .bearer_auth(token)
        .json(&serde_json::json!({"files":urls}))
        .send()
        .await
        .map_err(anyhow::Error::new)?;
    let status = response.status();
    let body: serde_json::Value = response.json().await.map_err(anyhow::Error::new)?;
    if !status.is_success() || body.get("success") != Some(&serde_json::Value::Bool(true)) {
        return Err(Error::Validation("CDN 清理失败".into()));
    }
    Ok(())
}
