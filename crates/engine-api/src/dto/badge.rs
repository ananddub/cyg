use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::repositories::models::BadgeDefinition;

/// The operator's badge: which counter it reads and what that counter has to
/// reach. A badge that names no live counter is simply never earned.
#[derive(Debug, Deserialize)]
pub struct BadgeDefinitionRequest {
    #[serde(default)]
    pub badge_key: String,
    pub title: String,
    #[serde(default)]
    pub detail: String,
    pub stat: String,
    pub threshold: i64,
    #[serde(default)]
    pub is_active: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct BadgeDefinitionRow {
    pub badge_key: String,
    pub title: String,
    pub detail: String,
    pub stat: String,
    pub threshold: i64,
    pub is_active: bool,
    pub update_time: DateTime<Utc>,
}

impl From<BadgeDefinition> for BadgeDefinitionRow {
    fn from(row: BadgeDefinition) -> Self {
        Self {
            badge_key: row.id,
            title: row.title,
            detail: row.detail,
            stat: row.stat,
            threshold: row.threshold,
            is_active: row.is_active,
            update_time: row.update_time,
        }
    }
}

/// One badge as the player sees it: the threshold, the counter's current value
/// and whether that value has reached it. Nothing here is stored per player.
#[derive(Debug, Serialize)]
pub struct BadgeRow {
    pub badge_key: String,
    pub title: String,
    pub detail: String,
    pub stat: String,
    pub threshold: i64,
    pub value: i64,
    pub earned: bool,
}

#[derive(Debug, Serialize)]
pub struct BadgeListResponse {
    pub badges: Vec<BadgeRow>,
}
