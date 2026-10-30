use serde::{Deserialize, Serialize};

use crate::db::bytes;
use crate::repositories::models::{GameplayEvent, MatchHistory};

#[derive(Debug, Default, Deserialize)]
pub struct HistoryParams {
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct DesyncParams {
    /// Narrow the list to one room; empty or absent means every room this
    /// process served.
    #[serde(default)]
    pub room: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

/// Everything needed to replay one room: the last state the host saved (base64,
/// absent when the room never persisted one) and the rows that describe what
/// happened around it.
#[derive(Debug, Serialize)]
pub struct ReplayBundle {
    pub room: String,
    /// Which backend produced `snapshot`, so a replayer knows how to read it.
    pub backend: Option<String>,
    /// Which save of the room these bytes are. Compare it against a match row's
    /// `snapshot_seq`: fewer means the host's last save never landed, so the
    /// replay base predates the match it is being shown with.
    pub snapshot_seq: Option<i64>,
    #[serde(with = "bytes::b64_opt")]
    pub snapshot: Option<Vec<u8>>,
    pub matches: Vec<MatchHistory>,
    pub events: Vec<GameplayEvent>,
}
