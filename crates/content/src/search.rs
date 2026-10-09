use crate::{Error, Result, query, search_index};
use chrono::{DateTime, Utc};
use sea_orm::{DatabaseConnection, FromQueryResult};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Serialize, ToSchema, FromQueryResult)]
pub struct SearchHit {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub published_at: DateTime<Utc>,
    pub rank: f32,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct SearchPage {
    pub items: Vec<SearchHit>,
    pub next_cursor: Option<String>,
    pub relaxed: bool,
}
#[derive(Serialize, Deserialize)]
struct Cursor {
    q: String,
    engine: String,
    config: String,
    relaxed: bool,
    rank: f32,
    time: DateTime<Utc>,
    id: Uuid,
}

pub trait SearchIndex {
    fn search(
        &self,
        q: &str,
        cursor: Option<&str>,
        limit: u32,
    ) -> impl std::future::Future<Output = Result<SearchPage>> + Send;
}
pub struct PostgresSearch<'a> {
    pub db: &'a DatabaseConnection,
}

fn flush_text(text: &str, result: &mut String) {
    if text == "OR" {
        result.push_str("OR");
    } else if let Some(word) = text.strip_prefix('-') {
        result.push_str(
            &search_index::segment(word, false)
                .split_whitespace()
                .map(|w| format!("-{w}"))
                .collect::<Vec<_>>()
                .join(" "),
        );
    } else {
        result.push_str(&search_index::segment(text, false));
    }
}

/// 保留 websearch 操作符与引号；分词文本片段，查询仍交给数据库安全解析。
fn query_text(q: &str) -> String {
    let mut result = String::new();
    let mut text = String::new();
    for c in q.chars() {
        if c == '"' || c.is_whitespace() {
            flush_text(&text, &mut result);
            text.clear();
            result.push(c);
        } else {
            text.push(c);
        }
    }
    flush_text(&text, &mut result);
    result
}

impl SearchIndex for PostgresSearch<'_> {
    async fn search(&self, q: &str, cursor: Option<&str>, limit: u32) -> Result<SearchPage> {
        let q = q.trim();
        if q.is_empty() || q.chars().count() > 300 {
            return Err(Error::Validation("搜索词限 1–300 字".into()));
        }
        let cursor = cursor
            .map(|c| {
                if c.len() > 4000 {
                    return Err(Error::Validation("游标过长".into()));
                }
                let cursor: Cursor = hex::decode(c)
                    .ok()
                    .and_then(|b| serde_json::from_slice(&b).ok())
                    .ok_or_else(|| Error::Validation("游标无效".into()))?;
                if cursor.q != q || !cursor.rank.is_finite() || cursor.rank < 0.0 {
                    return Err(Error::Validation("游标与查询不匹配".into()));
                }
                Ok(cursor)
            })
            .transpose()?;
        let limit = limit.clamp(1, 50) as usize;
        let config = search_index::settings(self.db).await?;
        if cursor
            .as_ref()
            .is_some_and(|c| c.engine != config.engine || c.config != config.config)
        {
            return Err(Error::Validation("检索配置已经变更，请重新搜索".into()));
        }
        let text = if config.engine == "app" {
            query_text(q)
        } else {
            q.to_owned()
        };
        let mut relaxed = cursor.as_ref().is_some_and(|c| c.relaxed);
        let mut items = fetch(
            self.db,
            &config.config,
            &text,
            relaxed,
            cursor.as_ref(),
            limit,
        )
        .await?;
        if cursor.is_none()
            && items.len() < limit
            && !q.contains(['"', '-'])
            && !q.split_whitespace().any(|w| w == "OR")
        {
            let loose = fetch(self.db, &config.config, &text, true, None, limit).await?;
            if loose.len() > items.len() {
                items = loose;
                relaxed = true;
            }
        }
        let next_cursor = if items.len() > limit {
            items.truncate(limit);
            let last = items.last().ok_or(Error::NotFound)?;
            Some(hex::encode(
                serde_json::to_vec(&Cursor {
                    q: q.to_owned(),
                    engine: config.engine.clone(),
                    config: config.config.clone(),
                    relaxed,
                    rank: last.rank,
                    time: last.published_at,
                    id: last.id,
                })
                .map_err(|e| Error::Validation(e.to_string()))?,
            ))
        } else {
            None
        };
        Ok(SearchPage {
            items,
            next_cursor,
            relaxed,
        })
    }
}

async fn fetch(
    db: &DatabaseConnection,
    config: &str,
    text: &str,
    relaxed: bool,
    cursor: Option<&Cursor>,
    limit: usize,
) -> Result<Vec<SearchHit>> {
    query::all(db,"WITH q AS (SELECT CASE WHEN $3::boolean THEN replace(plainto_tsquery($1::regconfig,$2)::text,' & ',' | ')::tsquery ELSE websearch_to_tsquery($1::regconfig,$2) END AS query), ranked AS (SELECT p.id,p.slug,p.title,p.summary,p.published_at,ts_rank_cd(p.search_vector,q.query) AS rank FROM posts p,q WHERE p.status='published' AND p.deleted_at IS NULL AND p.published_at<=now() AND p.search_vector @@ q.query) SELECT * FROM ranked WHERE ($4::real IS NULL OR (rank,published_at,id)<($4,$5::timestamptz,$6::uuid)) ORDER BY rank DESC,published_at DESC,id DESC LIMIT $7",vec![config.to_owned().into(),text.to_owned().into(),relaxed.into(),cursor.map(|c|c.rank).into(),cursor.map(|c|c.time).into(),cursor.map(|c|c.id).into(),((limit+1) as i64).into()]).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_websearch_syntax() {
        let text = query_text("\"分布式 博客\" OR rust -设计");
        assert!(text.contains("\"分布式 博客\" OR rust -设计"), "{text}");
    }
}
