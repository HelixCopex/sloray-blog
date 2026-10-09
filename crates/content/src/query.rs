use crate::Result;
use sea_orm::{ConnectionTrait, DbBackend, FromQueryResult, Statement, Value};

pub async fn one<T: FromQueryResult, C: ConnectionTrait>(
    db: &C,
    sql: &str,
    values: Vec<Value>,
) -> Result<Option<T>> {
    db.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        sql,
        values,
    ))
    .await?
    .map(|row| T::from_query_result(&row, "").map_err(Into::into))
    .transpose()
}

pub async fn all<T: FromQueryResult, C: ConnectionTrait>(
    db: &C,
    sql: &str,
    values: Vec<Value>,
) -> Result<Vec<T>> {
    db.query_all_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        sql,
        values,
    ))
    .await?
    .into_iter()
    .map(|row| T::from_query_result(&row, "").map_err(Into::into))
    .collect()
}

pub async fn exec<C: ConnectionTrait>(db: &C, sql: &str, values: Vec<Value>) -> Result<u64> {
    Ok(db
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            sql,
            values,
        ))
        .await?
        .rows_affected())
}
