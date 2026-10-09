//! 配置：内置默认值 → 运行时配置文件 → `SLORAY_*` 环境变量（后者覆盖前者）。
//!
//! 与早期版本相比，最关键的变化是**允许「未配置」状态**：
//! 没有数据库连接串时服务照常启动，只是进入 setup 模式。
//! 这是把部署流程从"SSH 上去手改 env 文件"变成
//! "装好 → 浏览器打开 `/setup` 配一下"的前提。
//!
//! 配置文件用 **JSON** 而不是 TOML，理由很实际：这个文件里会写数据库密码和
//! S3 secret，必须保证任意字符都能被正确转义。`serde_json` 的转义经过海量使用
//! 验证；手写 TOML 字符串则要在引号、反斜杠、换行上自己保证正确。

use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
};

use figment::{
    Figment,
    providers::{Env, Format, Json, Serialized},
};
use serde::{Deserialize, Serialize};

/// 运行时配置文件的默认位置。
///
/// 放在 `/srv/sloray/shared/` 而不是 `/etc/sloray/`：systemd 单元用了
/// `ProtectSystem=strict`（全盘只读），只对这一个目录开 `ReadWritePaths`。
/// 让服务能写配置是引入 Web 配置界面的**唯一**代价，不要把口子扩大到 /etc。
pub const DEFAULT_CONFIG_FILE: &str = "/srv/sloray/shared/config.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Development,
    Production,
}

impl Environment {
    pub fn is_production(self) -> bool {
        matches!(self, Environment::Production)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// 人类可读，本地开发用。
    Pretty,
    /// 结构化 JSON，生产环境给 journald 采集用。
    Json,
}

/// 对象存储（S3 兼容 / Cloudflare R2）配置。
///
/// 全部字段可空：媒体功能到 M2 才上线，向导允许先跳过，
/// 之后再在 `/admin/settings` 里补。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct S3Config {
    pub endpoint: Option<String>,
    pub region: Option<String>,
    pub bucket: Option<String>,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
}

impl S3Config {
    /// 五个字段要么都有、要么都没有。半配状态是最容易让人困惑的失败模式，
    /// 所以宁可在校验时直接报错。
    pub fn is_complete(&self) -> bool {
        self.endpoint.is_some()
            && self.region.is_some()
            && self.bucket.is_some()
            && self.access_key_id.is_some()
            && self.secret_access_key.is_some()
    }

    pub fn is_empty(&self) -> bool {
        self.endpoint.is_none()
            && self.region.is_none()
            && self.bucket.is_none()
            && self.access_key_id.is_none()
            && self.secret_access_key.is_none()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub environment: Environment,
    pub log_format: LogFormat,
    pub bind_addr: SocketAddr,

    /// **未配置时为 `None`** —— 服务会以 setup 模式启动。
    pub database_url: Option<String>,
    pub database_max_connections: u32,
    pub database_acquire_timeout_secs: u64,
    /// 收到 SIGTERM 后允许在途请求完成的时间。
    pub shutdown_timeout_secs: u64,

    /// 对外站点地址（canonical / 媒体链接 / 将来发信里的链接）。
    ///
    /// SSR 从公开站点配置接口读取，构建期值仅用作不可达时的退路。
    pub public_site_url: Option<String>,

    pub s3: S3Config,
    pub media_public_base_url: Option<String>,
    /// 仅存公开派生图，必须与私有原图 bucket 分开。
    pub media_public_bucket: Option<String>,
    pub cloudflare_zone_id: Option<String>,
    pub cloudflare_api_token: Option<String>,
    pub cloudflare_api_base_url: Option<String>,

    /// 运行时配置文件的位置。不写进文件自身，也不从文件读取。
    #[serde(skip)]
    pub config_file: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            environment: Environment::Development,
            log_format: LogFormat::Pretty,
            // 默认只监听回环：生产由 Nginx 反向代理，API 不直接暴露公网。
            bind_addr: "127.0.0.1:8080"
                .parse()
                .expect("默认监听地址是合法的 SocketAddr"),
            database_url: None,
            database_max_connections: 10,
            database_acquire_timeout_secs: 5,
            shutdown_timeout_secs: 10,
            public_site_url: None,
            s3: S3Config::default(),
            media_public_base_url: None,
            media_public_bucket: None,
            cloudflare_zone_id: None,
            cloudflare_api_token: None,
            cloudflare_api_base_url: None,
            config_file: PathBuf::from(DEFAULT_CONFIG_FILE),
        }
    }
}

impl Config {
    /// 装配配置。`config_file` 不存在时静默跳过 —— 首次部署就是这种情况。
    pub fn load(config_file: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let config_file = config_file.into();

        let mut config: Config = Figment::new()
            .merge(Serialized::defaults(Config::default()))
            .merge(Json::file(&config_file))
            .merge(Env::prefixed("SLORAY_").map(|key| {
                // 保留 database_url 等字段的下划线，只对 S3 的扁平环境变量映射嵌套层。
                if let Some(field) = key.as_str().to_ascii_lowercase().strip_prefix("s3_") {
                    format!("s3.{field}").into()
                } else {
                    key.into()
                }
            }))
            .extract()?;

        config.config_file = config_file;
        config.normalize();
        config.validate()?;
        Ok(config)
    }

    /// 环境变量里的空字符串应当视为"未设置"。
    /// 否则 `SLORAY_DATABASE_URL=` 会让服务以为自己已配置，然后连不上库。
    fn normalize(&mut self) {
        fn non_empty(v: &Option<String>) -> Option<String> {
            v.as_ref()
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
        }
        self.database_url = non_empty(&self.database_url);
        self.public_site_url = non_empty(&self.public_site_url);
        self.media_public_base_url = non_empty(&self.media_public_base_url);
        self.media_public_bucket = non_empty(&self.media_public_bucket);
        self.cloudflare_zone_id = non_empty(&self.cloudflare_zone_id);
        self.cloudflare_api_token = non_empty(&self.cloudflare_api_token);
        self.cloudflare_api_base_url = non_empty(&self.cloudflare_api_base_url);
        self.s3.endpoint = non_empty(&self.s3.endpoint);
        self.s3.region = non_empty(&self.s3.region);
        self.s3.bucket = non_empty(&self.s3.bucket);
        self.s3.access_key_id = non_empty(&self.s3.access_key_id);
        self.s3.secret_access_key = non_empty(&self.s3.secret_access_key);
    }

    /// 把配置写到磁盘：先写临时文件、收紧权限、再 rename 覆盖。
    ///
    /// 不做"直接覆盖写"：写到一半崩溃会留下半截 JSON，
    /// 下次启动解析失败会退化成未配置状态 —— 相当于悄悄丢配置。
    /// 同一个文件系统内 `rename` 是原子的。
    pub fn save(&self) -> anyhow::Result<()> {
        let path: &Path = &self.config_file;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| anyhow::anyhow!("创建配置目录 {} 失败：{e}", parent.display()))?;
        }

        let json = serde_json::to_string_pretty(self)?;

        crate::atomic::write_private(path, json.as_bytes())?;

        Ok(())
    }

    /// 是否已完成初始配置（有数据库连接串即视为已配置）。
    pub fn is_configured(&self) -> bool {
        self.database_url.is_some()
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if let Some(site) = &self.public_site_url {
            let parsed = url::Url::parse(site)
                .map_err(|_| anyhow::anyhow!("站点地址必须是有效的绝对 URL"))?;
            if !["http", "https"].contains(&parsed.scheme())
                || parsed.host_str().is_none()
                || !parsed.username().is_empty()
                || parsed.password().is_some()
                || parsed.query().is_some()
                || parsed.fragment().is_some()
                || parsed.path() != "/"
            {
                anyhow::bail!("站点地址必须是 http(s) 源地址，不能带凭据、路径、查询或片段");
            }
        }
        // 注意：**不再**要求 database_url 必须存在 —— 那正是 setup 模式的入口。
        if let Some(url) = &self.database_url
            && !url.starts_with("postgres://")
            && !url.starts_with("postgresql://")
        {
            anyhow::bail!(
                "数据库连接串必须以 postgres:// 或 postgresql:// 开头（当前值不是合法的 Postgres 连接串）"
            );
        }

        if self.database_max_connections == 0 {
            anyhow::bail!("SLORAY_DATABASE_MAX_CONNECTIONS 必须大于 0");
        }

        if !self.s3.is_empty() && !self.s3.is_complete() {
            anyhow::bail!(
                "对象存储配置不完整：endpoint / region / bucket / access_key_id / secret_access_key 必须同时提供"
            );
        }
        if self.media_public_base_url.is_some() != self.media_public_bucket.is_some() {
            anyhow::bail!("媒体公开地址与公开 bucket 必须同时配置");
        }
        if self.media_public_bucket.is_some()
            && (!self.s3.is_complete() || self.media_public_bucket == self.s3.bucket)
        {
            anyhow::bail!("公开派生图必须使用与私有原图不同的 bucket，并提供完整 S3 配置");
        }

        for (name, value) in [
            ("S3 endpoint", &self.s3.endpoint),
            ("媒体地址", &self.media_public_base_url),
            ("CDN API 地址", &self.cloudflare_api_base_url),
        ] {
            if let Some(value) = value {
                let parsed = url::Url::parse(value).map_err(|_| anyhow::anyhow!("{name} 无效"))?;
                if !["http", "https"].contains(&parsed.scheme())
                    || parsed.host_str().is_none()
                    || !parsed.username().is_empty()
                    || parsed.password().is_some()
                    || parsed.query().is_some()
                    || parsed.fragment().is_some()
                {
                    anyhow::bail!("{name} 必须是无凭据、查询和片段的 http(s) 地址");
                }
            }
        }
        if self.cloudflare_zone_id.is_some() != self.cloudflare_api_token.is_some() {
            anyhow::bail!("CDN zone id 与 API token 必须同时配置");
        }
        if self
            .cloudflare_zone_id
            .as_ref()
            .is_some_and(|zone| zone.is_empty() || !zone.bytes().all(|b| b.is_ascii_alphanumeric()))
        {
            anyhow::bail!("CDN zone id 无效");
        }
        Ok(())
    }

    /// 生产环境下若仍用 pretty 日志，提醒一句（不致命，便于本地验证生产构建）。
    pub fn warn_on_risky_defaults(&self) {
        if self.environment.is_production() && self.log_format == LogFormat::Pretty {
            tracing::warn!("生产环境建议设置 SLORAY_LOG_FORMAT=json，便于日志采集与检索");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_and_unconfigured() {
        let config = Config::default();
        assert_eq!(config.bind_addr.port(), 8080);
        // 关键：默认值必须能通过校验 —— 未配置是合法状态，不是错误。
        assert!(config.validate().is_ok());
        assert!(!config.is_configured());
    }

    #[test]
    fn rejects_non_postgres_url_but_allows_absent_url() {
        let bad = Config {
            database_url: Some("mysql://localhost/sloray".into()),
            ..Config::default()
        };
        assert!(bad.validate().is_err());

        let good = Config {
            database_url: Some("postgres://user:pw@127.0.0.1:5432/sloray".into()),
            ..Config::default()
        };
        assert!(good.validate().is_ok());
        assert!(good.is_configured());
    }

    #[test]
    fn empty_env_values_are_treated_as_unset() {
        // `SLORAY_DATABASE_URL=` 不能把服务骗成"已配置"。
        let mut config = Config {
            database_url: Some("   ".into()),
            ..Config::default()
        };
        config.normalize();
        assert_eq!(config.database_url, None);
        assert!(!config.is_configured());
    }

    #[test]
    fn half_configured_s3_is_rejected() {
        let partial = Config {
            s3: S3Config {
                endpoint: Some("https://x.r2.cloudflarestorage.com".into()),
                ..S3Config::default()
            },
            ..Config::default()
        };
        assert!(
            partial.validate().is_err(),
            "只配 endpoint 是典型的困惑源，应当直接报错"
        );

        let full = Config {
            s3: S3Config {
                endpoint: Some("https://x.r2.cloudflarestorage.com".into()),
                region: Some("auto".into()),
                bucket: Some("b".into()),
                access_key_id: Some("k".into()),
                secret_access_key: Some("s".into()),
            },
            ..Config::default()
        };
        assert!(full.validate().is_ok());
        assert!(full.s3.is_complete());
    }

    #[test]
    fn config_file_field_is_not_serialized() {
        let json = serde_json::to_value(Config::default()).expect("Config 必须可序列化");
        assert_eq!(json["environment"], "development");
        assert_eq!(json["log_format"], "pretty");
        assert_eq!(json["bind_addr"], "127.0.0.1:8080");
        // config_file 描述的是"文件在哪"，不该被写进文件自己。
        assert!(json.get("config_file").is_none());
    }

    #[test]
    fn public_media_cannot_share_the_private_bucket() {
        let mut config = Config {
            s3: S3Config {
                endpoint: Some("https://objects.example.com".into()),
                region: Some("auto".into()),
                bucket: Some("originals".into()),
                access_key_id: Some("key".into()),
                secret_access_key: Some("secret".into()),
            },
            media_public_base_url: Some("https://media.example.com".into()),
            ..Config::default()
        };
        assert!(config.validate().is_err());
        config.media_public_bucket = Some("originals".into());
        assert!(config.validate().is_err());
        config.media_public_bucket = Some("variants".into());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn secrets_survive_a_json_roundtrip() {
        // 密码里带引号、反斜杠、换行、制表符、Unicode ——
        // 手写 TOML 拼字符串最容易在这里翻车。
        let nasty = "p@ss\"w\\ord\n换行\ttab 'quote'";
        let config = Config {
            database_url: Some(format!("postgres://u:{nasty}@h:5432/db")),
            ..Config::default()
        };

        let text = serde_json::to_string(&config).unwrap();
        let back: Config = serde_json::from_str(&text).unwrap();
        assert_eq!(back.database_url, config.database_url);
    }
}
