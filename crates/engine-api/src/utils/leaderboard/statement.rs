use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

pub(crate) struct Write<'a> {
    pub(crate) leaderboard_id: &'a str,
    pub(crate) owner_id: Uuid,
    pub(crate) username: Option<&'a str>,
    pub(crate) abs: (i64, i64),
    pub(crate) delta: (i64, i64),
    pub(crate) metadata: Option<Value>,
    pub(crate) expiry: DateTime<Utc>,
    pub(crate) operator: i16,
}
