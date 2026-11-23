use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::repositories::models::RulesDefinition;

#[derive(poem_openapi::Object, Debug, Deserialize, Default)]
pub struct RuleRequest {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub body: Option<serde_json::Value>,
}

#[derive(poem_openapi::Object, Debug, Serialize)]
pub struct RuleRow {
    pub key: String,
    pub game_id: Option<Uuid>,
    pub version: i64,
    pub sha256: String,
    pub is_active: bool,
    pub body: serde_json::Value,
    pub create_time: DateTime<Utc>,
}

impl From<RulesDefinition> for RuleRow {
    fn from(row: RulesDefinition) -> Self {
        Self {
            key: row.key,
            game_id: row.game_id,
            version: row.version,
            sha256: row.sha256,
            is_active: row.is_active,
            body: row.body,
            create_time: row.create_time,
        }
    }
}

#[derive(poem_openapi::Object, Debug, Deserialize, Default)]
pub struct RuleDryRun {
    #[serde(default)]
    pub user_id: Option<Uuid>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub body: Option<serde_json::Value>,
    #[serde(default)]
    pub context: Option<serde_json::Value>,
}

#[derive(poem_openapi::Object, Debug, Serialize)]
pub struct RuleDryRunResult {
    pub matched: bool,
    pub evaluated_against: serde_json::Value,
    /// What a match would do. Empty when the rule did not match, so the console
    /// cannot show a reward for a rule the player does not qualify for.
    pub actions: Vec<serde_json::Value>,
    pub error: Option<String>,
}

/// How many stored versions a rule delete removed.
#[derive(poem_openapi::Object, Debug, Serialize)]
pub struct DeletedRuleResponse {
    pub deleted: u64,
}
