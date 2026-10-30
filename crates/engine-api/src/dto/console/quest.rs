use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::repositories::models::{QuestDefinition, Season};

#[derive(Debug, Deserialize)]
pub struct QuestDefinitionRequest {
    #[serde(default)]
    pub quest_key: String,
    pub title: String,
    pub goal_kind: String,
    pub goal_amount: i64,
    #[serde(default)]
    pub reward: serde_json::Value,
    #[serde(default)]
    pub season: Option<String>,
    #[serde(default)]
    pub active_from: Option<DateTime<Utc>>,
    #[serde(default)]
    pub active_to: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct QuestDefinitionRow {
    pub quest_key: String,
    pub game_id: Option<Uuid>,
    pub title: String,
    pub goal_kind: String,
    pub goal_amount: i64,
    pub reward: serde_json::Value,
    pub season: Option<String>,
    pub active_from: DateTime<Utc>,
    pub active_to: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct SeasonRequest {
    #[serde(default)]
    pub season_key: String,
    pub title: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    #[serde(default)]
    pub reward_track: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct SeasonRow {
    pub season_key: String,
    pub game_id: Option<Uuid>,
    pub title: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub reward_track: serde_json::Value,
    pub update_time: DateTime<Utc>,
}

impl From<QuestDefinition> for QuestDefinitionRow {
    fn from(row: QuestDefinition) -> Self {
        Self {
            quest_key: row.id,
            game_id: row.game_id,
            title: row.title,
            goal_kind: row.goal_kind,
            goal_amount: row.goal_amount,
            reward: row.reward,
            season: row.season,
            active_from: row.active_from,
            active_to: row.active_to,
            update_time: row.update_time,
        }
    }
}

impl From<Season> for SeasonRow {
    fn from(row: Season) -> Self {
        Self {
            season_key: row.id,
            game_id: row.game_id,
            title: row.title,
            starts_at: row.starts_at,
            ends_at: row.ends_at,
            reward_track: row.reward_track,
            update_time: row.update_time,
        }
    }
}
