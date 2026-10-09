//! 固定窗口限流通过原子 upsert 跨实例共享；不保存原始 IP。
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, DbErr, Statement};

pub async fn private_hash(db: &DatabaseConnection, subject: &str) -> Result<Vec<u8>, DbErr> {
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "INSERT INTO privacy_secrets(name,value) VALUES('ip',$1) ON CONFLICT DO NOTHING",
        [crate::generate_token().into()],
    ))
    .await?;
    let row = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT value FROM privacy_secrets WHERE name='ip'",
        ))
        .await?
        .ok_or_else(|| DbErr::Custom("missing privacy salt".into()))?;
    let salt: String = row.try_get("", "value")?;
    Ok(crate::token_hash(&format!("{salt}:{subject}")))
}

/// `window` 秒内最多 `maximum` 次。失败请求也占用配额。
pub async fn allow(
    db: &DatabaseConnection,
    scope: &str,
    subject: &[u8],
    window: i64,
    maximum: i64,
) -> Result<bool, DbErr> {
    let row = db.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,
        "INSERT INTO request_limits(scope,subject,bucket,count,expires_at) VALUES($1,$2,floor(extract(epoch FROM now())/$3)::bigint,1,now()+($3*2)*interval '1 second') ON CONFLICT(scope,subject,bucket) DO UPDATE SET count=least(request_limits.count+1,$4+1) RETURNING count",
        vec![scope.to_owned().into(), subject.to_vec().into(), window.into(), maximum.into()])).await?
        .ok_or_else(|| DbErr::Custom("missing limit result".into()))?;
    Ok(row.try_get::<i64>("", "count")? <= maximum)
}
