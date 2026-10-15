//! Leaderboards (`/v2/leaderboard/*`).
//!
//! BaaS parity: `server/api_leaderboard.go`, `core_leaderboard.go`. A record sits in a
//! window bucket and a cursor names the record it was cut at, so both paging directions read the same index.

use auto_di::singleton;

use crate::ApiState;
use crate::repositories::db::Queries;
use crate::repositories::models::Leaderboard;
use crate::error::V2Error;

use crate::utils::leaderboard::types::NOT_FOUND;

pub mod delete;
pub mod haystack;
pub mod list;
pub mod neighbours;
pub mod page;
pub mod rank;
pub mod write;

// The tournament group restates its records over this listing, so keep the free
// helpers at the module root.
pub(crate) use list::records_list;
pub(crate) use neighbours::haystack;
pub(crate) use rank::rank_of;

pub struct LeaderboardService;

#[singleton]
impl LeaderboardService {
    fn new() -> Self {
        Self
    }
}

/// The pool the raw-sqlx record upserts run against, or 503 when
/// `[control.session] database_url` is empty.
pub(crate) fn pool(state: &ApiState) -> Result<&sqlx::PgPool, V2Error> {
    state.db.as_ref().ok_or_else(V2Error::no_db)
}

/// The board a route addresses, in the gateway's own words when it is missing.
pub(crate) async fn load_board(queries: &Queries, id: &str) -> Result<Leaderboard, V2Error> {
    queries.get_leaderboard(id).await.map_err(|e| match e {
        sqlx::Error::RowNotFound => V2Error::not_found(NOT_FOUND),
        other => V2Error::from_sqlx(other),
    })
}
