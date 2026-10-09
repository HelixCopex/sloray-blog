//! 统一错误类型与错误信封。
//!
//! 对外契约固定为：
//!
//! ```json
//! { "error": { "code": "not_found", "message": "resource not found" } }
//! ```
//!
//! `code` 是稳定的机器可读标识（前端据此分支），`message` 是给人看的。
//! **内部错误绝不外泄**：数据库错误、SQL 片段、栈信息只进日志。

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("resource not found")]
    NotFound,

    #[error("unauthorized")]
    Unauthorized,

    #[error("forbidden")]
    Forbidden,

    #[error("resource was changed or a unique value already exists")]
    Conflict,

    #[error("请求过于频繁，请稍后重试")]
    RateLimited,

    /// 实例还没完成初始配置。返回 503 而不是 500：
    /// 这不是故障，而是"请先去 /setup 配一下"的正常状态。
    #[error("该实例尚未完成初始配置")]
    SetupRequired,

    #[error("{0}")]
    Validation(String),

    #[error("{0}")]
    BadRequest(String),

    /// 数据库错误：只在日志里出现，响应体统一成 internal_error。
    #[error(transparent)]
    Database(#[from] sea_orm::DbErr),

    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct ErrorDetail {
    code: &'static str,
    message: String,
}

impl AppError {
    /// (HTTP 状态码, 稳定的机器可读 code)
    fn parts(&self) -> (StatusCode, &'static str) {
        match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            AppError::Conflict => (StatusCode::CONFLICT, "conflict"),
            AppError::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
            // 503 而不是 500：未配置不是故障，是"请先去 /setup"。
            AppError::SetupRequired => (StatusCode::SERVICE_UNAVAILABLE, "setup_required"),
            AppError::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, "validation_failed"),
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            AppError::Database(_) | AppError::Internal(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
            }
        }
    }

    /// 是否可以安全地把详细信息返回给客户端。
    fn is_client_safe(&self) -> bool {
        matches!(
            self,
            AppError::NotFound
                | AppError::Unauthorized
                | AppError::Forbidden
                | AppError::Conflict
                | AppError::RateLimited
                | AppError::SetupRequired
                | AppError::Validation(_)
                | AppError::BadRequest(_)
        )
    }
}

impl From<sloray_content::Error> for AppError {
    fn from(error: sloray_content::Error) -> Self {
        use sloray_content::Error;
        match error {
            Error::NotFound => Self::NotFound,
            Error::Forbidden => Self::Forbidden,
            Error::Conflict => Self::Conflict,
            Error::Validation(message) => Self::Validation(message),
            Error::Database(error) => {
                if matches!(
                    error.sql_err(),
                    Some(sea_orm::SqlErr::UniqueConstraintViolation(_))
                ) {
                    Self::Conflict
                } else {
                    Self::Database(error)
                }
            }
        }
    }
}

impl From<sloray_media::Error> for AppError {
    fn from(error: sloray_media::Error) -> Self {
        match error {
            sloray_media::Error::NotFound => Self::NotFound,
            sloray_media::Error::Disabled => Self::Validation("对象存储尚未配置".into()),
            sloray_media::Error::Validation(message) => Self::Validation(message),
            sloray_media::Error::Database(error) => Self::Database(error),
            sloray_media::Error::Internal(_) => {
                Self::Internal(anyhow::anyhow!("媒体对象操作失败，请检查连接与权限"))
            }
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = self.parts();

        if status.is_server_error() {
            // 5xx 必须留痕：这是排查线上问题的唯一线索。
            // 但 SetupRequired 是"未配置"这个正常状态，不该每次都刷 error 日志
            // （否则未配置的实例会把 journald 灌满，真问题反而被淹没）。
            if matches!(self, AppError::SetupRequired) {
                tracing::debug!(code, "实例尚未配置");
            } else {
                tracing::error!(error = %self, code, "request failed");
            }
        } else {
            tracing::debug!(error = %self, code, "request rejected");
        }

        let message = if self.is_client_safe() {
            self.to_string()
        } else {
            "internal server error".to_owned()
        };

        let body = ErrorBody {
            error: ErrorDetail { code, message },
        };

        let mut response = (status, Json(body)).into_response();
        if status == StatusCode::TOO_MANY_REQUESTS {
            response.headers_mut().insert(
                axum::http::header::RETRY_AFTER,
                axum::http::HeaderValue::from_static("900"),
            );
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_errors_expose_message_and_status() {
        let (status, code) = AppError::NotFound.parts();
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(code, "not_found");
        assert!(AppError::NotFound.is_client_safe());
    }

    #[test]
    fn server_errors_are_opaque() {
        // 数据库错误必须归类为 5xx，且不允许把细节回给客户端。
        let err = AppError::Database(sea_orm::DbErr::Custom(
            "relation \"users\" does not exist".into(),
        ));
        let (status, code) = err.parts();
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(code, "internal_error");
        assert!(!err.is_client_safe());
    }

    #[test]
    fn validation_maps_to_422() {
        let (status, code) = AppError::Validation("slug 不能为空".into()).parts();
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(code, "validation_failed");
    }
}
