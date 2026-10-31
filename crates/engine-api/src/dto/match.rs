use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::services::realtime::matches::MatchSnapshot;

#[derive(Debug, Serialize)]
pub struct MatchResponse {
    pub match_id: String,
    pub authoritative: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub size: i32,
    #[serde(skip_serializing_if = "zero")]
    pub tick_rate: i32,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub handler_name: String,
}

fn zero(value: &i32) -> bool {
    *value == 0
}

impl From<&MatchSnapshot> for MatchResponse {
    fn from(m: &MatchSnapshot) -> Self {
        Self {
            match_id: m.id.simple().to_string(),
            authoritative: false,
            label: (!m.label.is_empty()).then(|| m.label.clone()),
            size: m.members.len() as i32,
            tick_rate: m.tick_rate,
            handler_name: String::new(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct MatchListResponse {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub matches: Vec<MatchResponse>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ListMatchesParams {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub authoritative: Option<bool>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default, alias = "min_count")]
    pub min_size: Option<i64>,
    #[serde(default)]
    pub max_size: Option<i64>,
    #[serde(default)]
    pub query: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MatchmakerStatsResponse {
    pub ticket_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oldest_ticket_create_time: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub completions: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub matchsets: Vec<MatchsetStat>,
}

#[derive(Debug, Serialize)]
pub struct MatchsetStat {
    pub matchset: String,
    pub ticket_count: i64,
    pub oldest_create_time: DateTime<Utc>,
    pub avg_wait_secs: f64,
}
