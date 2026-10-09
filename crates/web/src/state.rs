//! 运行时配置与连接池以同一把锁切换，避免读取到新配置与旧连接池的组合。
use crate::error::AppError;
use sea_orm::DatabaseConnection;
use sloray_config::Config;
use std::sync::Arc;
use tokio::sync::RwLock;

struct Snapshot {
    config: Config,
    db: Option<DatabaseConnection>,
}

#[derive(Clone)]
pub struct AppState {
    snapshot: Arc<RwLock<Snapshot>>,
}

impl AppState {
    pub fn new(config: Config, db: Option<DatabaseConnection>) -> Self {
        Self {
            snapshot: Arc::new(RwLock::new(Snapshot { config, db })),
        }
    }

    pub async fn config(&self) -> Config {
        self.snapshot.read().await.config.clone()
    }

    pub async fn db(&self) -> Result<DatabaseConnection, AppError> {
        self.snapshot
            .read()
            .await
            .db
            .clone()
            .ok_or(AppError::SetupRequired)
    }

    pub async fn db_optional(&self) -> Option<DatabaseConnection> {
        self.snapshot.read().await.db.clone()
    }

    pub async fn is_configured(&self) -> bool {
        self.snapshot.read().await.config.is_configured()
    }

    pub async fn set_db(&self, db: Option<DatabaseConnection>) {
        self.snapshot.write().await.db = db;
    }

    pub async fn save_config(&self, config: Config) -> anyhow::Result<()> {
        let mut snapshot = self.snapshot.write().await;
        config.save()?;
        snapshot.config = config;
        Ok(())
    }

    pub async fn reconfigure(
        &self,
        config: Config,
        db: Option<DatabaseConnection>,
    ) -> anyhow::Result<()> {
        let mut snapshot = self.snapshot.write().await;
        config.save()?;
        snapshot.config = config;
        snapshot.db = db;
        Ok(())
    }

    /// 其他实例通过共享配置文件发现向导/设置写入；启动时 DB 不可达也会重试。
    pub async fn refresh(&self) -> anyhow::Result<()> {
        let previous = self.config().await;
        let mut config = Config::load(previous.config_file.clone())?;
        // 实际监听 socket 在启动时已经确定；共享配置不能覆盖实例专属的端口。
        config.bind_addr = previous.bind_addr;
        let needs_connection =
            config.database_url != previous.database_url || self.db_optional().await.is_none();
        let db = if needs_connection {
            match config.database_url.as_deref() {
                Some(url) => Some(sloray_store::connect_database(url, &config).await?),
                None => None,
            }
        } else {
            self.db_optional().await
        };
        let mut snapshot = self.snapshot.write().await;
        // 等待新连接期间可能刚好完成设置写入；不以过时结果覆盖它。
        if serde_json::to_value(&snapshot.config)? == serde_json::to_value(&previous)? {
            snapshot.config = config;
            snapshot.db = db;
        }
        Ok(())
    }
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState").finish_non_exhaustive()
    }
}
