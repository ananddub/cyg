//! `/v2/match*` — the realtime match list and the matchmaker queue stats, as the
//! player API answers them.
//!
//! The shapes mirror the reference server's `api.Match` / `api.MatchList` / `api.MatchmakerStats`.
//! Where our relay plane genuinely holds no field — a handler name, a completion
//! history — the field is left out rather than invented, so an absent key means the
//! server has no such fact.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::services::realtime::matches::MatchSnapshot;

/// `api.Match`, a relay match as the list reports it. `authoritative` is always
/// `false`: this plane runs relay matches only, and the filter that asks for
/// authoritative ones simply answers none.
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
            // Relay matches are not run by a named handler, so there is no name.
            handler_name: String::new(),
        }
    }
}

/// `api.MatchList`.
#[derive(Debug, Serialize)]
pub struct MatchListResponse {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub matches: Vec<MatchResponse>,
}

/// `GET /v2/match` query. `min_size` also answers to `min_count`, the spelling the
/// rest of this API's matchmaker vocabulary uses.
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

/// `api.MatchmakerStats`. `completions` is empty because the relay matchmaker keeps
/// no finished-ticket history; `matchsets` is the queue split per matchset, the one
/// grouping our data supports.
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

/// The queue for one matchset, including how long its tickets have waited on average.
#[derive(Debug, Serialize)]
pub struct MatchsetStat {
    pub matchset: String,
    pub ticket_count: i64,
    pub oldest_create_time: DateTime<Utc>,
    pub avg_wait_secs: f64,
}
