//! Wire shapes of the admin console (`/v2/console/*`).
//!
//! The console is its own surface: it borrows the `/v2` request plumbing (the
//! `{"message": …}` failure body, the same extractors) but none of its player
//! session shapes, because a console operator is not a player.

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
pub mod setting;
pub mod status;
pub mod storage;
pub mod user;
pub mod wasm;

use serde::Deserialize;

/// The `?limit=&offset=` every console list accepts. The defaults and ceiling are
/// the console's own, so a table cannot ask a database for the whole table.
#[derive(Debug, Default, Deserialize)]
pub struct Page {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

/// The largest page a console list will serve, whatever it is asked for.
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
