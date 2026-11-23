use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::repositories::models::LeaderboardRecord;
use crate::services::social::{meta_str, uid};
use crate::utils::leaderboard::wire::{int64, stamp};

fn flex_i64<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = Value::deserialize(deserializer)?;
    let text = match raw {
        Value::Null => return Ok(0),
        Value::String(s) => s,
        Value::Number(n) => n.to_string(),
        other => {
            return Err(serde::de::Error::custom(format!(
                "expected an int64, found {other}"
            )));
        }
    };
    text.parse::<i64>().map_err(serde::de::Error::custom)
}

#[derive(Debug, Default, Deserialize)]
pub struct RecordRequest {
    #[serde(default, deserialize_with = "flex_i64")]
    pub score: i64,
    #[serde(default, deserialize_with = "flex_i64")]
    pub subscore: i64,
    #[serde(default)]
    pub metadata: Option<String>,
    #[serde(default, deserialize_with = "flex_i64")]
    pub operator: i64,
}

#[derive(Debug, Serialize, poem_openapi::Object)]
pub struct RecordResponse {
    pub leaderboard_id: String,
    pub owner_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscore: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_score: Option<i32>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub metadata: String,
    pub create_time: String,
    pub update_time: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_num_score: Option<i32>,
}

impl RecordResponse {
    pub fn from_record(record: &LeaderboardRecord, rank: i64) -> RecordResponse {
        RecordResponse {
            leaderboard_id: record.leaderboard_id.clone(),
            owner_id: uid(&record.owner_id),
            username: record.username.clone(),
            score: int64(record.score),
            subscore: int64(record.subscore),
            num_score: (record.num_score != 0).then_some(record.num_score),
            metadata: meta_str(&record.metadata),
            create_time: stamp(record.create_time),
            update_time: stamp(record.update_time),
            expiry_time: (record.expiry_time.timestamp() != 0).then(|| stamp(record.expiry_time)),
            rank: int64(rank),
            max_num_score: (record.max_num_score != 0).then_some(record.max_num_score),
        }
    }
}

#[derive(Debug, Default, Serialize, poem_openapi::Object)]
pub struct RecordListResponse {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub records: Vec<RecordResponse>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub owner_records: Vec<RecordResponse>,
    #[serde(
        rename = "nextPageToken",
        alias = "next_cursor",
        skip_serializing_if = "String::is_empty"
    )]
    pub next_cursor: String,
    #[serde(
        rename = "prevPageToken",
        alias = "prev_cursor",
        skip_serializing_if = "String::is_empty"
    )]
    pub prev_cursor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank_count: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub limit: Option<String>,
    #[serde(default)]
    pub cursor: String,
    #[serde(default)]
    pub owner_ids: Option<Value>,
    #[serde(default)]
    pub expiry: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct HaystackQuery {
    #[serde(default)]
    pub limit: Option<String>,
    #[serde(default)]
    pub cursor: String,
    #[serde(default)]
    pub expiry: Option<String>,
}

/// A board as the game declared it. The wire shape is spelled out here instead
/// of being assembled in the service, so the spec can describe it.
#[derive(Debug, Serialize, poem_openapi::Object)]
pub struct BoardDefinition {
    pub id: String,
    pub title: String,
    pub description: String,
    pub sort_order: i16,
    pub operator: i16,
    pub reset_schedule: Option<String>,
    pub metadata: Value,
    pub authoritative: bool,
    pub category: i16,
    pub max_num_score: i32,
}

impl From<&crate::repositories::models::Leaderboard> for BoardDefinition {
    fn from(row: &crate::repositories::models::Leaderboard) -> Self {
        BoardDefinition {
            id: row.id.clone(),
            title: row.title.clone(),
            description: row.description.clone(),
            reset_schedule: row.reset_schedule.clone(),
            sort_order: row.sort_order,
            operator: row.operator,
            metadata: row.metadata.clone(),
            authoritative: row.authoritative,
            category: row.category,
            max_num_score: row.max_num_score,
        }
    }
}

#[derive(Debug, Serialize, poem_openapi::Object)]
pub struct BoardListResponse {
    pub leaderboards: Vec<BoardDefinition>,
    pub total: usize,
}

#[derive(Debug, Serialize, poem_openapi::Object)]
pub struct DeletedBoard {
    pub deleted: String,
}
