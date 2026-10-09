//! 媒体业务与后台对象操作，不依赖 HTTP。
#![forbid(unsafe_code)]

pub mod dto;
pub mod gc;
mod query;
mod storage;
pub mod uploads;
pub mod worker;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("媒体不可用")]
    NotFound,
    #[error("对象存储尚未配置")]
    Disabled,
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Database(#[from] sea_orm::DbErr),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
