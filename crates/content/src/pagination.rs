use crate::{Error, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
pub struct Cursor {
    pub time: DateTime<Utc>,
    pub id: Uuid,
    pub scope: String,
}

pub fn decode(value: Option<&str>, scope: &str) -> Result<Option<Cursor>> {
    value
        .map(|value| {
            if value.len() > 2000 {
                return Err(Error::Validation("游标过长".into()));
            }
            let cursor: Cursor = hex::decode(value)
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .ok_or_else(|| Error::Validation("游标无效".into()))?;
            if cursor.scope != scope {
                return Err(Error::Validation("游标与筛选不匹配".into()));
            }
            Ok(cursor)
        })
        .transpose()
}

pub fn encode(time: DateTime<Utc>, id: Uuid, scope: String) -> Result<String> {
    serde_json::to_vec(&Cursor { time, id, scope })
        .map(hex::encode)
        .map_err(|e| Error::Validation(e.to_string()))
}
