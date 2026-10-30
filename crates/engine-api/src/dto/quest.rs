use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::repositories::models::{PlayerQuest, QuestDefinition};

#[derive(Debug, Serialize)]
pub struct QuestRow {
    pub quest_key: String,
    pub title: String,
    pub goal_kind: String,
    pub goal_amount: i64,
    pub progress: i64,
    pub state: String,
    pub reward: serde_json::Value,
    pub season: Option<String>,
    pub requires_product: Option<String>,
    pub active_from: DateTime<Utc>,
    pub active_to: DateTime<Utc>,
}
#[derive(Debug, Serialize)]
pub struct QuestListResponse {
    pub quests: Vec<QuestRow>,
}

#[derive(Debug, Serialize)]
pub struct QuestClaimResponse {
    pub success: bool,
    pub message: String,
    pub quest: QuestRow,
    pub reward: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct DailyClaimResponse {
    pub success: bool,
    pub message: String,
    pub day: i64,
    pub reward: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct ClaimQuestRequest {
    pub quest_key: String,
}

/// One server-authored game event: the name picks the counter, the value is the
/// increment.
#[derive(Debug, Deserialize)]
pub struct SimEvent {
    pub user_id: Uuid,
    pub name: String,
    #[serde(default)]
    pub value: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct IngestEventsRequest {
    #[serde(default)]
    pub events: Vec<SimEvent>,
}

#[derive(Debug, Serialize)]
pub struct CompletedQuest {
    pub user_id: Uuid,
    pub quest_key: String,
}

#[derive(Debug, Serialize)]
pub struct IngestEventsResponse {
    pub success: bool,
    pub accepted: usize,
    pub completed: Vec<CompletedQuest>,
}

/// A definition joined to the asking player's counters. A quest nobody has
/// touched yet still lists, at zero and available.
impl QuestRow {
    pub(crate) fn new(
        definition: &QuestDefinition,
        progress: Option<&PlayerQuest>,
        counted: i64,
    ) -> Self {
        let claimed = progress.is_some_and(|row| row.state == "claimed");
        let progress = if claimed {
            progress.expect("a claimed row exists").progress
        } else {
            counted
        };
        Self {
            quest_key: definition.id.clone(),
            title: definition.title.clone(),
            goal_kind: definition.goal_kind.clone(),
            goal_amount: definition.goal_amount,
            state: if claimed {
                "claimed".to_string()
            } else if progress >= definition.goal_amount {
                "completed".to_string()
            } else if progress > 0 {
                "in_progress".to_string()
            } else {
                "available".to_string()
            },
            progress,
            reward: definition.reward.clone(),
            season: definition.season.clone(),
            requires_product: definition.requires_product.clone(),
            active_from: definition.active_from,
            active_to: definition.active_to,
        }
    }
}
