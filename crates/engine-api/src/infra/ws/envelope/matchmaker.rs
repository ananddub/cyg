//! Matchmaker request and reply payloads.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::presence::Presence;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchmakerAdd {
    pub min_count: i32,
    pub max_count: i32,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub string_properties: HashMap<String, String>,
    #[serde(default)]
    pub numeric_properties: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchmakerMatched {
    pub ticket: String,
    pub match_id: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub users: Vec<MatchmakerUser>,
    #[serde(default)]
    pub self_presence: MatchmakerUser,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MatchmakerUser {
    pub presence: Presence,
    #[serde(default)]
    pub party_id: Option<String>,
    #[serde(default)]
    pub string_properties: HashMap<String, String>,
    #[serde(default)]
    pub numeric_properties: HashMap<String, f64>,
}
