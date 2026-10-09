use crate::{Error, Result};
use aws_sdk_s3::{
    Client,
    config::{
        BehaviorVersion, Credentials, Region, RequestChecksumCalculation,
        ResponseChecksumValidation,
    },
    primitives::ByteStream,
};
use sloray_config::Config;
use std::time::Duration;

pub const MAX_BYTES: i64 = 10 * 1024 * 1024;
pub const WIDTHS: [i32; 5] = [320, 640, 960, 1280, 1920];
pub struct Storage {
    pub client: Client,
    pub bucket: String,
    pub public_bucket: String,
}

impl Storage {
    pub fn new(config: &Config) -> Result<Self> {
        let s3 = &config.s3;
        if !s3.is_complete() {
            return Err(Error::Disabled);
        }
        let cfg = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new(s3.region.clone().ok_or(Error::Disabled)?))
            .credentials_provider(Credentials::new(
                s3.access_key_id.clone().ok_or(Error::Disabled)?,
                s3.secret_access_key.clone().ok_or(Error::Disabled)?,
                None,
                None,
                "sloray-config",
            ))
            .endpoint_url(s3.endpoint.clone().ok_or(Error::Disabled)?)
            .force_path_style(true)
            .request_checksum_calculation(RequestChecksumCalculation::WhenRequired)
            .response_checksum_validation(ResponseChecksumValidation::WhenRequired)
            .timeout_config(
                aws_sdk_s3::config::timeout::TimeoutConfig::builder()
                    .operation_timeout(Duration::from_secs(30))
                    .build(),
            )
            .build();
        Ok(Self {
            client: Client::from_conf(cfg),
            bucket: s3.bucket.clone().ok_or(Error::Disabled)?,
            public_bucket: config.media_public_bucket.clone().ok_or(Error::Disabled)?,
        })
    }

    pub async fn read(&self, key: &str, maximum: usize, range: bool) -> Result<Vec<u8>> {
        let mut builder = self.client.get_object().bucket(&self.bucket).key(key);
        if range {
            builder = builder.range(format!("bytes=0-{}", maximum.saturating_sub(1)));
        }
        let mut body = builder.send().await.map_err(anyhow::Error::new)?.body;
        let mut bytes = Vec::new();
        while let Some(chunk) = body.next().await {
            let chunk = chunk.map_err(anyhow::Error::new)?;
            if bytes.len() + chunk.len() > maximum {
                return Err(Error::Validation("对象读取超出限制".into()));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }

    pub async fn remove(&self, key: &str) -> Result<()> {
        self.remove_from(&self.bucket, key).await
    }

    pub async fn remove_variant(&self, key: &str) -> Result<()> {
        self.remove_from(&self.public_bucket, key).await
    }

    async fn remove_from(&self, bucket: &str, key: &str) -> Result<()> {
        self.client
            .delete_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(anyhow::Error::new)?;
        Ok(())
    }

    pub async fn variant(&self, key: &str, bytes: Vec<u8>, immutable: bool) -> Result<()> {
        let result = self
            .client
            .put_object()
            .bucket(&self.public_bucket)
            .key(key)
            .if_none_match("*")
            .content_type("image/webp")
            .cache_control(if immutable {
                "public,max-age=31536000,immutable"
            } else {
                "public,no-cache"
            })
            .body(ByteStream::from(bytes))
            .send()
            .await;
        match result {
            Ok(_) => Ok(()),
            Err(e) if e.raw_response().is_some_and(|r| r.status().as_u16() == 412) => Ok(()),
            Err(e) => Err(anyhow::Error::new(e).into()),
        }
    }
}

pub fn variant_key(id: uuid::Uuid, width: i32) -> String {
    format!("media/{id}/v1/{width}.webp")
}
pub fn url(config: &Config, key: &str) -> Option<String> {
    config
        .media_public_base_url
        .as_ref()
        .map(|base| format!("{}/{}", base.trim_end_matches('/'), key))
}
