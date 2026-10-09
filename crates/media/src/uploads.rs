use crate::{
    Error, Result,
    dto::*,
    query,
    storage::{self, MAX_BYTES, Storage},
};
use aws_sdk_s3::presigning::PresigningConfig;
use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::{DateTime, Utc};
use sea_orm::{ConnectionTrait, DatabaseConnection, FromQueryResult, TransactionTrait};
use sloray_config::Config;
use uuid::Uuid;

#[derive(FromQueryResult)]
pub(crate) struct Row {
    pub id: Uuid,
    pub object_key: String,
    pub mime: String,
    pub size_bytes: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub variants_generated_at: Option<DateTime<Utc>>,
    pub verified: bool,
    pub created_at: DateTime<Utc>,
    pub sha256: Vec<u8>,
}

impl Row {
    pub fn dto(self, config: &Config) -> Media {
        let ready = self.verified && self.variants_generated_at.is_some();
        let mut widths = storage::WIDTHS
            .into_iter()
            .filter(|w| self.width.is_some_and(|width| *w <= width))
            .collect::<Vec<_>>();
        if widths.is_empty()
            && let Some(width) = self.width
        {
            widths.push(width);
        }
        let variants = if ready {
            widths
                .into_iter()
                .filter_map(|width| {
                    storage::url(config, &storage::variant_key(self.id, width))
                        .map(|url| Variant { width, url })
                })
                .collect()
        } else {
            vec![]
        };
        Media {
            id: self.id,
            mime: self.mime,
            size_bytes: self.size_bytes,
            width: self.width,
            height: self.height,
            ready,
            variants,
            created_at: self.created_at,
        }
    }
}

pub async fn get<C: ConnectionTrait>(db: &C, config: &Config, id: Uuid) -> Result<Media> {
    let row: Row = query::one(
        db,
        "SELECT * FROM media WHERE id=$1 AND deleted_at IS NULL",
        vec![id.into()],
    )
    .await?
    .ok_or(Error::NotFound)?;
    Ok(row.dto(config))
}

pub async fn list(db: &DatabaseConnection, config: &Config) -> Result<Vec<Media>> {
    let rows: Vec<Row> = query::all(
        db,
        "SELECT * FROM media WHERE deleted_at IS NULL ORDER BY created_at DESC,id DESC LIMIT 100",
        vec![],
    )
    .await?;
    Ok(rows.into_iter().map(|row| row.dto(config)).collect())
}

pub async fn presign(
    db: &DatabaseConnection,
    config: &Config,
    owner: Uuid,
    input: PresignInput,
) -> Result<Presigned> {
    let extension = match input.content_type.as_str() {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        _ => return Err(Error::Validation("只支持 PNG、JPEG、WebP 图片".into())),
    };
    if !(1..=MAX_BYTES).contains(&input.size_bytes) {
        return Err(Error::Validation("图片最大 10 MiB".into()));
    }
    let hash = hex::decode(&input.sha256)
        .ok()
        .filter(|h| h.len() == 32)
        .ok_or_else(|| Error::Validation("SHA-256 无效".into()))?;
    let storage = Storage::new(config)?;
    if let Some(row)=query::one::<Row,_>(db,"SELECT * FROM media WHERE sha256=$1 AND deleted_at IS NULL AND verified AND variants_generated_at IS NOT NULL LIMIT 1",vec![hash.clone().into()]).await? {
        return Ok(Presigned {upload_id:None,upload_url:None,headers:Default::default(),expires_at:None,media:Some(row.dto(config))});
    }
    let id = Uuid::now_v7();
    let now = Utc::now();
    let key = format!("private/{}/{id}.{extension}", now.format("%Y/%m"));
    let expires = now + chrono::Duration::minutes(10);
    let signed = storage
        .client
        .put_object()
        .bucket(&storage.bucket)
        .key(&key)
        .content_type(&input.content_type)
        .if_none_match("*")
        .checksum_sha256(STANDARD.encode(&hash))
        .presigned(
            PresigningConfig::expires_in(std::time::Duration::from_secs(600))
                .map_err(anyhow::Error::new)?,
        )
        .await
        .map_err(anyhow::Error::new)?;
    let headers = signed
        .headers()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .filter(|(k, _)| k != "host")
        .collect();
    query::exec(db,"INSERT INTO media_uploads(id,owner_id,object_key,mime,size_bytes,sha256,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7)",vec![id.into(),owner.into(),key.into(),input.content_type.into(),input.size_bytes.into(),hash.into(),expires.into()]).await?;
    Ok(Presigned {
        upload_id: Some(id),
        upload_url: Some(signed.uri().to_owned()),
        headers,
        expires_at: Some(expires),
        media: None,
    })
}

pub async fn register(
    db: &DatabaseConnection,
    config: &Config,
    owner: Uuid,
    input: RegisterInput,
) -> Result<Media> {
    #[derive(FromQueryResult)]
    struct Ticket {
        id: Uuid,
        object_key: String,
        mime: String,
        size_bytes: i64,
        sha256: Vec<u8>,
        expires_at: DateTime<Utc>,
        completed_at: Option<DateTime<Utc>>,
    }
    let txn = db.begin().await?;
    let ticket: Ticket = query::one(
        &txn,
        "SELECT * FROM media_uploads WHERE id=$1 AND owner_id=$2 FOR UPDATE",
        vec![input.upload_id.into(), owner.into()],
    )
    .await?
    .ok_or(Error::NotFound)?;
    if ticket.completed_at.is_some() {
        return get(&txn, config, ticket.id).await;
    }
    if ticket.expires_at < Utc::now() {
        return Err(Error::Validation("上传许可已过期，请重新选择图片".into()));
    }
    let storage = Storage::new(config)?;
    let head = storage
        .client
        .head_object()
        .bucket(&storage.bucket)
        .key(&ticket.object_key)
        .send()
        .await
        .map_err(|error| {
            if error
                .raw_response()
                .is_some_and(|r| r.status().as_u16() == 404)
            {
                Error::Validation("上传对象尚不存在，请完成上传后重试".into())
            } else {
                Error::Internal(anyhow::Error::new(error))
            }
        })?;
    let valid = head.content_length() == Some(ticket.size_bytes)
        && ticket.size_bytes <= MAX_BYTES
        && head.content_type() == Some(ticket.mime.as_str());
    let bytes = if valid {
        storage.read(&ticket.object_key, 512, true).await?
    } else {
        vec![]
    };
    let actual = image::guess_format(&bytes)
        .ok()
        .map(|format| format.to_mime_type());
    if !valid || actual != Some(ticket.mime.as_str()) {
        storage.remove(&ticket.object_key).await?;
        return Err(Error::Validation(
            "实际大小或图片类型与上传声明不匹配".into(),
        ));
    }
    query::exec(&txn,"INSERT INTO media(id,object_key,mime,size_bytes,sha256,uploaded_by) VALUES($1,$2,$3,$4,$5,$6)",vec![ticket.id.into(),ticket.object_key.into(),ticket.mime.into(),ticket.size_bytes.into(),ticket.sha256.into(),owner.into()]).await?;
    query::exec(&txn,"INSERT INTO outbox(id,topic,payload) VALUES($1,'media.generate',jsonb_build_object('id',$2::text))",vec![Uuid::now_v7().into(),ticket.id.to_string().into()]).await?;
    query::exec(
        &txn,
        "UPDATE media_uploads SET completed_at=now() WHERE id=$1",
        vec![ticket.id.into()],
    )
    .await?;
    let media = get(&txn, config, ticket.id).await?;
    txn.commit().await?;
    Ok(media)
}

pub async fn delete(db: &DatabaseConnection, id: Uuid) -> Result<()> {
    let txn = db.begin().await?;
    let row: Row = query::one(
        &txn,
        "SELECT * FROM media WHERE id=$1 AND deleted_at IS NULL FOR UPDATE",
        vec![id.into()],
    )
    .await?
    .ok_or(Error::NotFound)?;
    query::exec(
        &txn,
        "UPDATE media SET deleted_at=now() WHERE id=$1",
        vec![id.into()],
    )
    .await?;
    query::exec(&txn,"UPDATE posts SET cover_media_id=NULL,updated_at=now(),version=version+1 WHERE cover_media_id=$1",vec![id.into()]).await?;
    let payload = serde_json::json!({"id":id,"key":row.object_key,"width":row.width});
    query::exec(
        &txn,
        "INSERT INTO outbox(id,topic,payload) VALUES($1,'media.delete',$2)",
        vec![Uuid::now_v7().into(), payload.into()],
    )
    .await?;
    txn.commit().await?;
    Ok(())
}
