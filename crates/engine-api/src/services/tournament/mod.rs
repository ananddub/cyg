use auto_di::singleton;

use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::repositories::models::Leaderboard;

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

pub(crate) async fn load_tournament(queries: &Queries, id: &str) -> Result<Leaderboard, V2Error> {
    queries.get_tournament(id).await.map_err(|e| match e {
        crate::db::DbError::NotFound => V2Error::not_found(NOT_FOUND),
        other => {
            tracing::info!("[v2] tournament load: {other}");
            V2Error::not_found(NOT_FOUND)
        }
    })
}
