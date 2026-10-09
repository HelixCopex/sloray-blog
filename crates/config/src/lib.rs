//! 配置 crate。
//!
//! 职责：把配置从"内置默认值 → 运行时配置文件 → `SLORAY_*` 环境变量"三层装配起来，
//! 校验它，并能把它**原子地**写回磁盘。
//!
//! 不依赖任何其它内部 crate —— 这是刻意的：配置是所有东西的最底层，
//! 一旦它开始依赖别的东西，循环依赖就会立刻出现。

#![forbid(unsafe_code)]

pub mod atomic;
pub mod config;

pub use config::{Config, DEFAULT_CONFIG_FILE, Environment, LogFormat, S3Config};

/// 把数据库连接串里的密码换成 `***`，用于展示给用户看。
///
/// 设置页要显示"当前连的是哪个库"，但**绝不能在页面里回显密码** ——
/// 那会让任何一次截屏、录屏、浏览器自动填充都变成凭据泄漏。
pub fn mask_database_url(url: &str) -> String {
    // 形如 postgres://user:password@host:port/db
    let Some(scheme_end) = url.find("://") else {
        return "***".to_owned();
    };
    let (scheme, rest) = url.split_at(scheme_end + 3);
    let Some(at) = rest.rfind('@') else {
        return url.to_owned();
    };
    let (creds, host) = rest.split_at(at);
    match creds.split_once(':') {
        Some((user, _password)) => format!("{scheme}{user}:***{host}"),
        None => format!("{scheme}{creds}{host}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_password_but_keeps_the_rest_recognisable() {
        let masked = mask_database_url("postgres://sloray:s3cret@127.0.0.1:5432/blog");
        assert_eq!(masked, "postgres://sloray:***@127.0.0.1:5432/blog");
        assert!(!masked.contains("s3cret"), "密码绝不能出现在展示串里");
    }

    #[test]
    fn mask_handles_password_containing_at_and_colon() {
        // 密码里可以有 @ 和 :，所以必须从右边找最后一个 @。
        let masked = mask_database_url("postgres://u:p@ss:w0rd@host:5432/db");
        assert_eq!(masked, "postgres://u:***@host:5432/db");
    }

    #[test]
    fn mask_is_total_for_garbage_input() {
        assert_eq!(mask_database_url("not a url"), "***");
        assert_eq!(
            mask_database_url("postgres://host/db"),
            "postgres://host/db"
        );
    }
}
