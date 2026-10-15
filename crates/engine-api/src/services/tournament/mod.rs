//! Tournaments (`/v2/tournament*`).
//!
//! BaaS parity: `server/api_tournament.go`, `server/core_tournament.go`. A tournament is a
//! `leaderboard` row with a non-zero `duration`, and its records sit in the round window.

use auto_di::singleton;

use crate::repositories::db::Queries;
use crate::repositories::models::Leaderboard;
use crate::error::V2Error;

use crate::utils::tournament::types::NOT_FOUND;

pub mod delete;
pub mod haystack;
pub mod join;
pub mod list;
pub mod records;
mod submit;
pub mod write;

pub struct TournamentService;

#[singleton]
impl TournamentService {
    fn new() -> Self {
        Self
    }
}

/// The tournament a route addresses. Rows whose duration is zero are plain
/// leaderboards, which no tournament route will agree to talk about.
pub(crate) async fn load_tournament(queries: &Queries, id: &str) -> Result<Leaderboard, V2Error> {
    queries.get_tournament(id).await.map_err(|e| match e {
        sqlx::Error::RowNotFound => V2Error::not_found(NOT_FOUND),
        other => {
            eprintln!("[v2] tournament load: {other}");
            V2Error::not_found(NOT_FOUND)
        }
    })
}
