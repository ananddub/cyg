use auto_di::singleton;

use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::repositories::models::Leaderboard;

use crate::utils::leaderboard::types::NOT_FOUND;

pub mod delete;
pub mod haystack;
pub mod list;
pub mod neighbours;
pub mod page;
pub mod rank;
pub mod write;

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

pub(crate) async fn load_board(queries: &Queries, id: &str) -> Result<Leaderboard, V2Error> {
    queries.get_leaderboard(id).await.map_err(|e| match e {
        crate::db::DbError::NotFound => V2Error::not_found(NOT_FOUND),
        other => V2Error::from(other),
    })
}
