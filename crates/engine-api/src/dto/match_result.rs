use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// What the host says one player did. `stats` are the counters this match raised
/// for them, named the way an event names a counter.
#[derive(Debug, Deserialize)]
pub struct ParticipantResult {
    pub user_id: Uuid,
    pub score: i64,
    #[serde(default)]
    pub subscore: i64,
    #[serde(default)]
    pub stats: BTreeMap<String, i64>,
    /// Attached to the leaderboard record; an object or nothing.
    #[serde(default)]
    pub metadata: Option<Value>,
}

/// The host's word on how a match ended, for one leaderboard at a time.
#[derive(Debug, Deserialize)]
pub struct MatchResultRequest {
    /// The board these scores post to. Without one the counters still move and
    /// no score is published.
    #[serde(default)]
    pub leaderboard: Option<String>,
    pub participants: Vec<ParticipantResult>,
}

/// One player as the control plane recorded them: the score now on the board and
/// the counters this result left behind.
#[derive(Debug, Serialize)]
pub struct RecordedScore {
    pub user_id: Uuid,
    pub score: i64,
    pub subscore: i64,
    pub counters: BTreeMap<String, i64>,
}

#[derive(Debug, Serialize)]
pub struct MatchResultResponse {
    pub success: bool,
    pub match_id: Uuid,
    pub leaderboard: Option<String>,
    pub recorded: Vec<RecordedScore>,
}
