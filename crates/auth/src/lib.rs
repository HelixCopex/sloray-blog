//! 最小可用的鉴权：argon2id 密码哈希 + 数据库会话表。
//!
//! 为什么不用 JWT：见 ADR-0007。一句话 —— 会话需要能被撤销
//! （退出登录、封禁、改密码后失效），而 JWT 要做到这点必须引入黑名单，
//! 等于把状态又加回来，却失去了它唯一的优势。
//!
//! 为什么 Cookie 自己拼而不用 axum-extra：只需要读一个键、写一个 Set-Cookie，
//! 手写 20 行比多一个依赖更划算，而且完全可测。

#![forbid(unsafe_code)]

pub mod accounts;
pub mod limits;

use std::time::Duration;

use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};
use axum::http::{HeaderMap, header};
use chrono::{Duration as ChronoDuration, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    Set,
};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use sloray_store::entity::{sessions, users};

/// 会话 Cookie 名。
pub const SESSION_COOKIE: &str = "sloray_session";

/// 会话有效期。写操作需要重新登录的频率由它决定。
pub const SESSION_TTL_SECS: i64 = 14 * 24 * 60 * 60;

// ---------------------------------------------------------------- 密码

/// 用 argon2id 生成 PHC 字符串（salt 由 argon2 自己随机生成）。
pub fn hash_password(password: &str) -> anyhow::Result<String> {
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes())
        .map_err(|e| anyhow::anyhow!("密码哈希失败：{e}"))?;
    Ok(hash.to_string())
}

/// 校验密码。任何解析失败都返回 false —— 不要把错误细节泄漏给调用方。
pub fn verify_password(password: &str, phc: &str) -> bool {
    match PasswordHash::new(phc) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(err) => {
            tracing::warn!(error = %err, "密码哈希格式无法解析");
            false
        }
    }
}

// ---------------------------------------------------------------- token

/// 生成 244 位随机 token（两个 UUIDv4 拼起来，底层是 getrandom）。
///
/// 刻意不引入 `rand`：uuid 已经在依赖树里，而它足以提供密码学强度的随机数。
pub fn generate_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

/// 会话 token 只存哈希。数据库被读走 ≠ 会话被接管。
pub fn token_hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

/// SHA-256 哈希后的恒定时间比较，避免 token 校验泄漏前缀信息。
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && bool::from(a.ct_eq(b))
}

// ---------------------------------------------------------------- Cookie

fn cookie_attrs(secure: bool) -> String {
    // HttpOnly：JS 读不到，降低 XSS 窃取会话的影响面。
    // SameSite=Lax：配合 ADR-0002 的同源部署，足够阻断跨站表单提交。
    let mut attrs = String::from("; Path=/; HttpOnly; SameSite=Lax");
    if secure {
        attrs.push_str("; Secure");
    }
    attrs
}

/// 构造登录成功时的 Set-Cookie。
pub fn session_cookie(token: &str, secure: bool) -> String {
    format!(
        "{SESSION_COOKIE}={token}; Max-Age={SESSION_TTL_SECS}{}",
        cookie_attrs(secure)
    )
}

/// 构造登出时的 Set-Cookie（立即过期）。
pub fn clear_session_cookie(secure: bool) -> String {
    format!("{SESSION_COOKIE}=; Max-Age=0{}", cookie_attrs(secure))
}

/// 从 Cookie 头里取指定键的值。
///
/// 手写解析要小心前缀误匹配：`sloray_session_x=1` 不该命中 `sloray_session`。
/// 这里是靠"紧跟 `=`"来排除的。
pub fn read_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in raw.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix(name)
            && let Some(value) = rest.strip_prefix('=')
        {
            return Some(value.to_owned());
        }
    }
    None
}

// ---------------------------------------------------------------- 会话存取

/// 建立会话，返回**明文** token（只在这一刻存在于内存里，之后库里只有哈希）。
pub async fn create_session(
    db: &DatabaseConnection,
    user_id: Uuid,
    user_agent: Option<String>,
) -> Result<String, sea_orm::DbErr> {
    let token = generate_token();
    let now = Utc::now();

    let model = sessions::ActiveModel {
        id: Set(Uuid::now_v7()),
        token_hash: Set(token_hash(&token)),
        user_id: Set(user_id),
        expires_at: Set((now + ChronoDuration::seconds(SESSION_TTL_SECS)).fixed_offset()),
        // 出于隐私考虑不存明文 IP；这里留 None，等有明确需求再存哈希。
        ip_hash: Set(None),
        user_agent: Set(user_agent),
        created_at: Set(now.fixed_offset()),
    };
    sessions::Entity::insert(model).exec(db).await?;

    Ok(token)
}

/// 用会话 token 查出用户。过期或不存在都返回 None。
pub async fn user_for_session(
    db: &DatabaseConnection,
    token: &str,
) -> Result<Option<users::Model>, sea_orm::DbErr> {
    let hash = token_hash(token);

    let session = sessions::Entity::find()
        .filter(sessions::Column::TokenHash.eq(hash))
        .one(db)
        .await?;

    let Some(session) = session else {
        return Ok(None);
    };

    // 过期判断放在应用层：这样不必依赖数据库时钟，行为在任何部署下都一致。
    if session.expires_at < Utc::now().fixed_offset() {
        // 顺手清理，避免过期行无限堆积。
        let _ = sessions::Entity::delete_by_id(session.id).exec(db).await;
        return Ok(None);
    }

    users::Entity::find_by_id(session.user_id).one(db).await
}

/// 销毁会话（登出）。
pub async fn destroy_session(db: &DatabaseConnection, token: &str) -> Result<(), sea_orm::DbErr> {
    sessions::Entity::delete_many()
        .filter(sessions::Column::TokenHash.eq(token_hash(token)))
        .exec(db)
        .await?;
    Ok(())
}

/// 按邮箱查用户（登录用）。邮箱在库里一律小写。
pub async fn find_user_by_email(
    db: &DatabaseConnection,
    email: &str,
) -> Result<Option<users::Model>, sea_orm::DbErr> {
    users::Entity::find()
        .filter(users::Column::Email.eq(normalize_email(email)))
        .one(db)
        .await
}

/// 邮箱归一化：数据库上有 `CHECK (email = lower(email))`，
/// 不先转小写会直接违反约束。
pub fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

/// 创建用户。
pub async fn create_user<C: sea_orm::ConnectionTrait>(
    db: &C,
    email: &str,
    password: &str,
    display_name: &str,
    role: &str,
) -> anyhow::Result<users::Model> {
    let now = Utc::now().fixed_offset();
    let model = users::ActiveModel {
        id: Set(Uuid::now_v7()),
        email: Set(normalize_email(email)),
        password_hash: Set({
            let password = password.to_owned();
            tokio::task::spawn_blocking(move || hash_password(&password)).await??
        }),
        display_name: Set(display_name.to_owned()),
        role: Set(role.to_owned()),
        created_at: Set(now),
        updated_at: Set(now),
    };
    Ok(model.insert(db).await?)
}

/// 统计管理员数量。setup 用它判断"是否已经有人初始化过"。
pub async fn admin_count<C: sea_orm::ConnectionTrait>(db: &C) -> Result<u64, sea_orm::DbErr> {
    users::Entity::find()
        .filter(users::Column::Role.eq(users::ROLE_ADMIN))
        .count(db)
        .await
}

/// 在同一事务的咨询锁下检查并创建首位管理员，避免两个 setup 实例抢跑。
pub async fn create_first_admin(
    db: &DatabaseConnection,
    email: &str,
    password: &str,
    display_name: &str,
) -> anyhow::Result<Option<users::Model>> {
    use sea_orm::{ConnectionTrait, TransactionTrait};
    let txn = db.begin().await?;
    txn.execute_unprepared("SELECT pg_advisory_xact_lock(1269876543)")
        .await?;
    let user = if admin_count(&txn).await? == 0 {
        Some(create_user(&txn, email, password, display_name, users::ROLE_ADMIN).await?)
    } else {
        None
    };
    txn.commit().await?;
    Ok(user)
}

/// 密码强度下限。刻意保持宽松（这是自建博客，不是银行），
/// 但拒绝最常见的弱口令，避免出现 `admin/admin`。
pub fn validate_password(password: &str) -> Result<(), String> {
    if password.len() > 1024 {
        return Err("密码过长（最多 1024 字节）".into());
    }
    const WEAK: &[&str] = &[
        "admin", "password", "123456", "12345678", "sloray", "qwerty",
    ];
    if password.chars().count() < 8 {
        return Err("密码至少需要 8 个字符".to_owned());
    }
    if WEAK.contains(&password.to_lowercase().as_str()) {
        return Err("这个密码太常见了，换一个".to_owned());
    }
    Ok(())
}

/// 给会话 Cookie 用的 `secure` 判定。
///
/// 不能无条件加 `Secure`：首次配置常常是在 `http://<ip>:8080/setup` 上完成的，
/// 加了 `Secure` 浏览器直接丢弃 Cookie，登录会莫名其妙失败。
pub fn cookie_is_secure(public_site_url: Option<&str>) -> bool {
    public_site_url.is_some_and(|u| u.starts_with("https://"))
}

/// 会话空闲时长的可读表示，给设置页展示用。
pub fn session_ttl() -> Duration {
    Duration::from_secs(SESSION_TTL_SECS as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn password_hash_roundtrip() {
        let phc = hash_password("correct horse battery staple").unwrap();
        // PHC 字符串应当标明算法与参数，便于将来平滑升级参数。
        assert!(phc.starts_with("$argon2id$"), "实际：{phc}");
        assert!(verify_password("correct horse battery staple", &phc));
        assert!(!verify_password("wrong password", &phc));
    }

    #[test]
    fn verify_rejects_garbage_hash_without_panicking() {
        assert!(!verify_password("whatever", "not-a-phc-string"));
        assert!(!verify_password("whatever", ""));
    }

    #[test]
    fn tokens_are_unique_and_long_enough() {
        let a = generate_token();
        let b = generate_token();
        assert_ne!(a, b);
        // 2 个 UUIDv4 的 hex = 64 字符。
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn token_hash_is_stable_and_hides_plaintext() {
        let t = "abc123";
        assert_eq!(token_hash(t), token_hash(t));
        assert_eq!(token_hash(t).len(), 32);
        assert!(!token_hash(t).windows(3).any(|w| w == b"abc"));
    }

    #[test]
    fn constant_time_eq_behaves_like_eq_for_equal_lengths() {
        assert!(constant_time_eq(b"abcd", b"abcd"));
        assert!(!constant_time_eq(b"abcd", b"abce"));
        assert!(!constant_time_eq(b"abcd", b"abc"));
    }

    #[test]
    fn reads_only_the_exact_cookie_name() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("a=1; sloray_session=TOKEN; sloray_session_x=2"),
        );
        assert_eq!(
            read_cookie(&headers, SESSION_COOKIE).as_deref(),
            Some("TOKEN")
        );
        assert_eq!(read_cookie(&headers, "a").as_deref(), Some("1"));
        // 前缀相同但名字不同的不能被误命中。
        assert_eq!(read_cookie(&headers, "sloray").as_deref(), None);
        assert_eq!(read_cookie(&headers, "missing").as_deref(), None);
    }

    #[test]
    fn cookie_flags_follow_https() {
        let insecure = session_cookie("t", false);
        assert!(insecure.contains("HttpOnly"));
        assert!(insecure.contains("SameSite=Lax"));
        assert!(
            !insecure.contains("Secure"),
            "http 场景加 Secure 会导致登录静默失败"
        );

        assert!(session_cookie("t", true).contains("; Secure"));
        assert!(clear_session_cookie(true).contains("Max-Age=0"));
        assert!(cookie_is_secure(Some("https://blog.example.com")));
        assert!(!cookie_is_secure(Some("http://127.0.0.1:8080")));
        assert!(!cookie_is_secure(None));
    }

    #[test]
    fn password_policy_rejects_the_obvious_ones() {
        assert!(validate_password("admin").is_err());
        assert!(validate_password("password").is_err());
        assert!(validate_password("short").is_err());
        assert!(validate_password("a-perfectly-fine-passphrase").is_ok());
    }

    #[test]
    fn email_is_normalised_before_hitting_the_check_constraint() {
        // 数据库上有 CHECK (email = lower(email))，这里必须保证先转小写。
        assert_eq!(normalize_email("  Admin@Example.COM "), "admin@example.com");
    }
}
