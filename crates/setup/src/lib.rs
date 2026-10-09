//! 首次配置（setup）模式 —— "WordPress 式部署流程"的核心。
//!
//! 设计目标：**部署完什么都不用改文件**。服务以未配置状态启动，
//! 用一次性 token 打开 `/setup` 填数据库/S3/站点地址，配置写回磁盘并即时生效。
//!
//! # 三个状态
//!
//! | 状态 | 含义 | `/setup` 是否可用 |
//! |---|---|---|
//! | `Required` | 还没配数据库 | 可用（需 token） |
//! | `Recoverable` | 配了但连不上（密码错、库没起） | 可用（需 token）—— 救急通道 |
//! | `Disabled` | 已配置且数据库可用 | **不可用** |
//!
//! `Recoverable` 是刻意保留的：否则一个写错的数据库密码就会让实例
//! 只能靠 SSH 上去手改文件才能恢复，而"不用上服务器就能修"正是这套流程的价值。
//!
//! # token 的生命周期
//!
//! token 随机生成、写 0600 文件、并打印到 journald。
//! 它**只在 Required / Recoverable 状态下被接受**；实例健康时形同不存在。
//! 文件本身会一直保留 —— 那正是上面这条救急通道的前提。想换掉它就跑
//! `sloray setup-token --rotate`。

#![forbid(unsafe_code)]

pub mod systemd;
use std::path::{Path, PathBuf};

use sea_orm::{ConnectOptions, Database, DatabaseConnection};

use sloray_auth as auth;
use sloray_config::{Config, DEFAULT_CONFIG_FILE};

/// setup token 的文件名（与运行时配置文件同目录）。
pub const SETUP_TOKEN_FILE: &str = "setup-token";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupMode {
    Required,
    Recoverable,
    Disabled,
}

impl SetupMode {
    /// setup 页面是否应当响应请求。
    pub fn is_open(self) -> bool {
        !matches!(self, SetupMode::Disabled)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            SetupMode::Required => "required",
            SetupMode::Recoverable => "recoverable",
            SetupMode::Disabled => "disabled",
        }
    }
}

/// 由"配置里有没有数据库"以及"数据库连不连得上"共同决定。
pub async fn mode(config: &Config, db_reachable: bool) -> SetupMode {
    if !config.is_configured() {
        SetupMode::Required
    } else if db_reachable {
        SetupMode::Disabled
    } else {
        SetupMode::Recoverable
    }
}

/// 有连接串不等于完成初始化：provision 可能已经建库，但还没有表或管理员。
pub async fn database_mode(config: &Config, db: Option<&DatabaseConnection>) -> SetupMode {
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let Some(db) = db else {
        return mode(config, false).await;
    };
    let table = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT to_regclass('public.users') IS NOT NULL AS present".to_owned(),
        ))
        .await;
    match table
        .and_then(|row| row.ok_or_else(|| sea_orm::DbErr::Custom("no result".into())))
        .and_then(|row| row.try_get::<bool>("", "present"))
    {
        Ok(false) => SetupMode::Required,
        Ok(true) => match auth::admin_count(db).await {
            Ok(0) => SetupMode::Required,
            Ok(_) => SetupMode::Disabled,
            Err(_) => SetupMode::Recoverable,
        },
        Err(_) => SetupMode::Recoverable,
    }
}

/// token 文件的位置：与配置文件同目录，这样它落在同一个可写目录里
/// （systemd 只对这个目录开了 `ReadWritePaths`）。
pub fn token_path(config_file: &Path) -> PathBuf {
    config_file
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(SETUP_TOKEN_FILE)
}

/// 默认配置文件路径下的 token 路径。
pub fn default_token_path() -> PathBuf {
    token_path(Path::new(DEFAULT_CONFIG_FILE))
}

pub fn read_token(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

/// 写入 token，权限 0600。先写临时文件再 rename，避免留下半截内容。
pub fn write_token(path: &Path, token: &str) -> anyhow::Result<()> {
    sloray_config::atomic::write_private(path, format!("{token}\n").as_bytes())?;
    Ok(())
}

/// 读取已有的 token；没有就生成一个并落盘。
pub fn load_or_create_token(path: &Path) -> anyhow::Result<String> {
    if let Some(existing) = read_token(path) {
        return Ok(existing);
    }
    let token = auth::generate_token();
    if sloray_config::atomic::create_private(path, format!("{token}\n").as_bytes())? {
        Ok(token)
    } else {
        read_token(path).ok_or_else(|| anyhow::anyhow!("已有 setup token 文件不可读或为空"))
    }
}

/// 恒定时间比较，避免 token 校验被逐字节猜。
pub fn verify_token(provided: &str, expected: &str) -> bool {
    auth::constant_time_eq(provided.trim().as_bytes(), expected.as_bytes())
}

/// 尝试连接数据库并 ping 一次。
///
/// 返回 `Result<(), String>` 而不是带连接的错误类型：这里的错误文本要**直接展示
/// 给用户**（在 setup 表单上），所以必须是人话，而且不能包含连接串本身。
pub async fn probe_database(url: &str, config: &Config) -> Result<DatabaseConnection, String> {
    let mut options = ConnectOptions::new(url.to_owned());
    options
        .max_connections(config.database_max_connections)
        .min_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(
            config.database_acquire_timeout_secs,
        ))
        .sqlx_logging(false)
        .set_application_name("sloray");

    let db = Database::connect(options)
        .await
        .map_err(|e| friendly_db_error(&e.to_string()))?;

    db.ping()
        .await
        .map_err(|e| friendly_db_error(&e.to_string()))?;

    Ok(db)
}

/// 把 sqlx 的原始错误翻译成"运维能照着做"的提示。
///
/// 刻意把原始错误里的细节去掉后再返回：这些字符串会渲染到 HTML 页面上，
/// 而页面的访问者此刻只有 setup token 这一层保护。
fn friendly_db_error(raw: &str) -> String {
    let lower = raw.to_lowercase();
    if lower.contains("password authentication failed") {
        "密码认证失败：用户名或密码不对".to_owned()
    } else if lower.contains("does not exist") {
        "数据库或角色不存在：请先创建数据库（createdb）".to_owned()
    } else if lower.contains("connection refused") || lower.contains("timed out") {
        "连不上数据库：确认 Postgres 正在运行、地址与端口正确".to_owned()
    } else if lower.contains("pool timed out") {
        "连接池超时：确认 Postgres 正在运行且监听地址正确".to_owned()
    } else {
        "连接失败：请检查数据库服务、连接配置与服务日志".to_owned()
    }
}

/// 跑迁移（带咨询锁单飞）。
pub async fn run_migrations(db: &DatabaseConnection) -> anyhow::Result<()> {
    sloray_migration::migrate_up_with_lock(db).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_modes_map_to_openness() {
        assert!(SetupMode::Required.is_open());
        assert!(SetupMode::Recoverable.is_open());
        assert!(!SetupMode::Disabled.is_open());
    }

    #[tokio::test]
    async fn mode_is_required_without_database_url() {
        let config = Config::default();
        assert_eq!(mode(&config, false).await, SetupMode::Required);
        // 即使数据库"可达"（这里只是传 true），没配就是没配。
        assert_eq!(mode(&config, true).await, SetupMode::Required);
    }

    #[tokio::test]
    async fn mode_is_recoverable_when_configured_but_unreachable() {
        let config = Config {
            database_url: Some("postgres://u:p@127.0.0.1:1/db".into()),
            ..Config::default()
        };
        assert_eq!(mode(&config, false).await, SetupMode::Recoverable);
        assert_eq!(mode(&config, true).await, SetupMode::Disabled);
    }

    #[test]
    fn token_roundtrip_and_verification() {
        let dir = std::env::temp_dir().join(format!("sloray-token-{}", auth::generate_token()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(SETUP_TOKEN_FILE);

        let token = load_or_create_token(&path).unwrap();
        // 第二次调用必须拿到同一个 token，而不是重新生成
        // （否则重启服务就会把上一张纸条作废）。
        assert_eq!(load_or_create_token(&path).unwrap(), token);
        assert!(verify_token(&token, &token));
        assert!(
            verify_token(&format!("  {token}  "), &token),
            "应容忍复制粘贴带空白"
        );
        assert!(!verify_token("wrong", &token));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "token 文件必须只有属主可读");
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn token_path_sits_next_to_the_config_file() {
        assert_eq!(
            token_path(Path::new("/srv/sloray/shared/config.json")),
            PathBuf::from("/srv/sloray/shared/setup-token")
        );
    }

    #[test]
    fn db_errors_are_translated_to_actionable_text() {
        // 关键是**不要把原始错误直接抛给页面**：里面可能含主机名、端口、用户名。
        let msg = friendly_db_error("password authentication failed for user \"sloray\"");
        assert!(msg.contains("密码"));
        assert!(!msg.contains("sloray"), "不该回显用户名：{msg}");
    }
}
