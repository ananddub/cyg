use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// One merged row of the operator's player timeline. Every source table maps
/// into this shape; `kind` is what the UI renders the row with.
#[derive(Debug, Serialize)]
pub struct TimelineEntry {
    pub id: String,
    pub kind: String,
    pub at: DateTime<Utc>,
    pub summary: String,
    #[serde(default)]
    pub detail: Value,
}

#[derive(Debug, Serialize)]
pub struct TimelinePage {
    pub account_id: Uuid,
    pub entries: Vec<TimelineEntry>,
    /// Pass as `before` on the next request; `null` means the feed is done.
    pub next_cursor: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct TimelineQuery {
    #[serde(default)]
    pub before: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub game_id: Option<Uuid>,
}
