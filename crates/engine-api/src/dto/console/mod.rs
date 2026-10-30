pub mod account;
pub mod audit;
pub mod auth;
pub mod games;
pub mod group;
pub mod iap;
pub mod leaderboard;
#[path = "match.rs"]
pub mod matches;
pub mod notification;
pub mod quest;
pub mod setting;
pub mod status;
pub mod storage;
pub mod user;
pub mod wasm;

use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Default, Deserialize)]
pub struct Page {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
    #[serde(default)]
    pub game_id: Option<Uuid>,
}

#[derive(Debug, Default, Deserialize)]
pub struct GameScope {
    #[serde(default)]
    pub game_id: Option<Uuid>,
}

impl GameScope {
    pub fn id(&self) -> Option<Uuid> {
        self.game_id
    }
}

const MAX_LIMIT: i64 = 100;
const DEFAULT_LIMIT: i64 = 25;

impl Page {
    pub fn limit(&self) -> i64 {
        self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
    }

    pub fn offset(&self) -> i64 {
        self.offset.unwrap_or(0).max(0)
    }
}
