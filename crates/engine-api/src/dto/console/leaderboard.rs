use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::repositories::models::{Leaderboard, LeaderboardRecord};

#[derive(Debug, Default, Deserialize)]
pub struct TournamentWriteRequest {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub operator: Option<i16>,
    #[serde(default)]
    pub sort_order: Option<i16>,
    #[serde(default)]
    pub duration: Option<i32>,
    #[serde(default)]
    pub start_time: Option<DateTime<Utc>>,
    #[serde(default)]
    pub end_time: Option<DateTime<Utc>>,
    #[serde(default)]
    pub reset_schedule: Option<String>,
    #[serde(default)]
    pub authoritative: Option<bool>,
    #[serde(default)]
    pub join_required: Option<bool>,
    #[serde(default)]
    pub max_size: Option<i32>,
    #[serde(default)]
    pub max_num_score: Option<i32>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct LeaderboardRow {
    pub id: String,
    pub title: String,
    pub description: String,
    pub kind: &'static str,
    pub category: i16,
    pub operator: i16,
    pub sort_order: i16,
    pub reset_schedule: String,
    pub authoritative: bool,
    pub join_required: bool,
    pub metadata: serde_json::Value,
    pub size: i32,
    pub max_size: i32,
    pub max_num_score: i32,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub duration: i32,
    pub create_time: DateTime<Utc>,
}

impl From<Leaderboard> for LeaderboardRow {
    fn from(l: Leaderboard) -> Self {
        Self {
            id: l.id,
            title: l.title,
            description: l.description,
            kind: if l.category == 1 {
                "tournament"
            } else {
                "leaderboard"
            },
            category: l.category,
            operator: l.operator,
            sort_order: l.sort_order,
            reset_schedule: l.reset_schedule.unwrap_or_default(),
            authoritative: l.authoritative,
            join_required: l.join_required,
            metadata: l.metadata,
            size: l.size,
            max_size: l.max_size,
            max_num_score: l.max_num_score,
            start_time: l.start_time,
            end_time: l.end_time,
            duration: l.duration,
            create_time: l.create_time,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LeaderboardRecordRow {
    pub leaderboard_id: String,
    pub owner_id: Uuid,
    pub username: String,
    pub score: i64,
    pub subscore: i64,
    pub num_score: i32,
    pub metadata: serde_json::Value,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
    pub expiry_time: DateTime<Utc>,
}

impl From<LeaderboardRecord> for LeaderboardRecordRow {
    fn from(r: LeaderboardRecord) -> Self {
        Self {
            leaderboard_id: r.leaderboard_id,
            owner_id: r.owner_id,
            username: r.username.unwrap_or_default(),
            score: r.score,
            subscore: r.subscore,
            num_score: r.num_score,
            metadata: r.metadata,
            create_time: r.create_time,
            update_time: r.update_time,
            expiry_time: r.expiry_time,
        }
    }
}
