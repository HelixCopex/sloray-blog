//! HTTP 层 crate：路由、HTML 渲染、服务启动与日志。
//!
//! 这一层是**唯一**依赖全部业务 crate 的地方，也是唯一知道 HTTP 的地方。
//! 反过来不成立：config / store / auth / setup 都不依赖它 ——
//! 依赖方向单向，才谈得上"功能分立"。

#![forbid(unsafe_code)]
pub mod openapi;

pub mod error;
pub mod html;
pub mod routes;
pub mod server;
pub mod state;
pub mod telemetry;

pub use error::{AppError, Result};
pub use state::AppState;
