use super::*;

pub struct DefineLeaderboardParams {
    pub id: String,
    pub title: String,
    pub description: String,
    pub operator: i16,
    pub sort_order: i16,
    pub reset_schedule: String,
    pub metadata: serde_json::Value,
    pub authoritative: bool,
    pub category: i16,
    pub max_num_score: i32,
}

pub struct GetLeaderboardRecordParams {
    pub leaderboard_id: String,
    pub owner_id: Uuid,
    pub expiry_time: DateTime<Utc>,
}

pub struct GetLeaderboardRecordRankDescParams {
    pub leaderboard_id: String,
    pub expiry_time: DateTime<Utc>,
    pub score: i64,
    pub subscore: i64,
    pub owner_id: Uuid,
}

pub type GetLeaderboardRecordRankAscParams = GetLeaderboardRecordRankDescParams;

pub struct FirstLeaderboardRecordsAscParams {
    pub leaderboard_id: String,
    pub expiry_time: DateTime<Utc>,
    pub limit: i64,
}

pub type FirstLeaderboardRecordsDescParams = FirstLeaderboardRecordsAscParams;

pub struct GreaterLeaderboardRecordsAscParams {
    pub leaderboard_id: String,
    pub expiry_time: DateTime<Utc>,
    pub score: i64,
    pub subscore: i64,
    pub owner_id: Uuid,
    pub limit: i64,
}

pub type LessLeaderboardRecordsDescParams = GreaterLeaderboardRecordsAscParams;

pub struct ListLeaderboardRecordsForOwnerParams {
    pub leaderboard_id: String,
    pub expiry_time: DateTime<Utc>,
    pub owner_id: Vec<Uuid>,
}

pub type DeleteLeaderboardRecordParams = GetLeaderboardRecordParams;

pub struct UpdateTournamentParams {
    pub id: String,
    pub title: String,
    pub description: String,
    pub operator: i16,
    pub sort_order: i16,
    pub reset_schedule: Option<String>,
    pub metadata: serde_json::Value,
    pub authoritative: bool,
    pub duration: i32,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub join_required: bool,
    pub max_size: i32,
    pub max_num_score: i32,
}

pub(super) const UPDATE_TOURNAMENT: &str = "UPDATE type::record('leaderboard', $id)
SET title = $title, description = $description, operator = $operator,
    sort_order = $sort_order, reset_schedule = $reset_schedule, metadata = $metadata,
    authoritative = $authoritative, duration = $duration, start_time = $start_time,
    end_time = $end_time, join_required = $join_required, max_size = $max_size,
    max_num_score = $max_num_score
WHERE category = 1
RETURN AFTER";

pub(super) const RECORD_COLUMNS: &str = "leaderboard_id, owner_id, username, score, subscore, num_score, \
     max_num_score, metadata, create_time, update_time, expiry_time";

pub(super) fn new_leaderboard(id: &str, now: DateTime<Utc>) -> Leaderboard {
    Leaderboard {
        id: id.to_string(),
        authoritative: false,
        sort_order: 1,
        operator: 0,
        reset_schedule: None,
        metadata: json!({}),
        create_time: now,
        category: 0,
        description: String::new(),
        duration: 0,
        end_time: time::epoch(),
        join_required: false,
        max_size: 100_000_000,
        max_num_score: 1_000_000,
        title: String::new(),
        size: 0,
        start_time: now,
        enable_ranks: Some(true),
        game_id: None,
    }
}
