//! 内容业务；HTTP 路由只编排这些操作，DTO 与数据库结构分离。
#![forbid(unsafe_code)]

pub mod activity_dto;
pub mod comments;
pub mod dto;
pub mod page_dto;
pub mod pages;
mod pagination;
pub mod posts;
mod query;
pub mod revisions;
pub mod search;
pub mod search_index;
pub mod site;
pub mod stats;
pub mod theme_dto;
pub mod themes;
pub mod worker;

use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Copy)]
pub struct Actor {
    pub id: Uuid,
    pub admin: bool,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("not found")]
    NotFound,
    #[error("forbidden")]
    Forbidden,
    #[error("article was changed by another editor")]
    Conflict,
    #[error("{0}")]
    Validation(String),
    #[error(transparent)]
    Database(#[from] sea_orm::DbErr),
}

pub type Result<T> = std::result::Result<T, Error>;
