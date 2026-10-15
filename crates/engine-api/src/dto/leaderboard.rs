//! `/v2/leaderboard*` and `/v2/tournament*` records — the record and record-list
//! shapes.
//!
//! The gateway prints enums as numbers and keys in snake_case, and omits
//! default-valued members: int64/uint64 as decimal strings, a zero scalar or empty
//! repeated field absent rather than `null`, and a timestamp from `Unix()` seconds
//! only, so nanos never appear.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::repositories::models::LeaderboardRecord;
use crate::services::social::{meta_str, uid};
use crate::utils::leaderboard::wire::{int64, stamp};

/// The gateway accepts a quoted or a bare int64 and rejects anything else.
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

/// The `record` message that `POST /v2/leaderboard/{id}` posts as its body.
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

/// A record with no scores left prints as `{}`: every zero-valued scalar is absent.
#[derive(Debug, Serialize)]
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

/// Empty repeated fields and a zero rank count are absent, which is how
/// BaaS answers an empty page.
#[derive(Debug, Default, Serialize)]
pub struct RecordListResponse {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub records: Vec<RecordResponse>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub owner_records: Vec<RecordResponse>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub next_cursor: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub prev_cursor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank_count: Option<String>,
}

/// `GET /v2/leaderboard/{leaderboard_id}` query. `limit` and `expiry` are nullable
/// fields, so "absent" and "zero" differ and both are legal; a repeated `owner_ids`
/// may be comma-separated or listed.
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

/// `GET /v2/leaderboard/{leaderboard_id}/owner/{owner_id}` query.
#[derive(Debug, Default, Deserialize)]
pub struct HaystackQuery {
    #[serde(default)]
    pub limit: Option<String>,
    #[serde(default)]
    pub cursor: String,
    #[serde(default)]
    pub expiry: Option<String>,
}
