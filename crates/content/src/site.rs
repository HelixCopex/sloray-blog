//! 编辑期站点标识；与部署配置分开保存，带乐观锁及数据库 outbox。
use crate::{Actor, Error, Result, query, themes::administrator};
use sea_orm::{DatabaseConnection, FromQueryResult};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, FromQueryResult)]
pub struct SiteIdentity {
    pub site_name: String,
    pub favicon_url: Option<String>,
    pub version: i32,
}
impl Default for SiteIdentity {
    fn default() -> Self {
        Self {
            site_name: "静线数据库".into(),
            favicon_url: None,
            version: 1,
        }
    }
}
pub fn validate(name: &str, favicon: Option<&str>) -> Result<()> {
    if name.trim().is_empty() || name.chars().count() > 100 || name.chars().any(char::is_control) {
        return Err(Error::Validation(
            "站点名称须为 1–100 字，不能包含控制字符".into(),
        ));
    }
    if let Some(url) = favicon {
        // 只允许同源绝对路径或 HTTPS URL，拒绝协议相对地址、脚本和内嵌数据。
        if url.len() > 2048
            || url
                .chars()
                .any(|c| c.is_whitespace() || c.is_control() || c == '\\')
            || !(url.starts_with('/') && !url.starts_with("//")
                || url.starts_with("https://") && url.len() > 8)
        {
            return Err(Error::Validation(
                "Favicon 请使用 / 开头的同源路径或 HTTPS 图片地址".into(),
            ));
        }
    }
    Ok(())
}
pub async fn get(db: &DatabaseConnection) -> Result<SiteIdentity> {
    query::one(
        db,
        "SELECT site_name,favicon_url,version FROM site_identity WHERE id=1",
        vec![],
    )
    .await?
    .ok_or(Error::NotFound)
}
pub async fn save(
    db: &DatabaseConnection,
    actor: Actor,
    input: SiteIdentity,
) -> Result<SiteIdentity> {
    administrator(actor)?;
    let name = input.site_name.trim();
    let favicon = input
        .favicon_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    validate(name, favicon)?;
    query::one(db, "UPDATE site_identity SET site_name=$1,favicon_url=$2,version=version+1 WHERE id=1 AND version=$3 RETURNING site_name,favicon_url,version", vec![name.into(),favicon.map(String::from).into(),input.version.into()])
        .await?.ok_or(Error::Conflict)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_urls_are_passive_and_bounded() {
        for url in [
            "javascript:alert(1)",
            "data:image/svg+xml,x",
            "//evil.test/icon",
            "/\\evil",
            "https://a\n.test",
        ] {
            assert!(validate("站点", Some(url)).is_err());
        }
        for url in ["/favicon.svg", "https://cdn.example/icon.webp"] {
            assert!(validate("站点", Some(url)).is_ok());
        }
        assert!(validate(" ", None).is_err());
    }
}
