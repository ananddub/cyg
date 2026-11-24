use auto_di::singleton;
use std::sync::Arc;

use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::repositories::models::Leaderboard;

use crate::utils::leaderboard::types::NOT_FOUND;

pub mod define;
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

/// The boards surface.
///
/// It holds the plane, injected at construction, rather than taking `&ApiState`
/// from every caller: the dependency is in the service's own shape.
pub struct LeaderboardService {
    pub(super) plane: Arc<crate::services::plane::Plane>,
}

#[singleton]
impl LeaderboardService {
    pub fn new(plane: Arc<crate::services::plane::Plane>) -> Self {
        Self { plane }
    }
}

pub(crate) async fn load_board(queries: &Queries, id: &str) -> Result<Leaderboard, V2Error> {
    queries.get_leaderboard(id).await.map_err(|e| match e {
        crate::db::DbError::NotFound => V2Error::not_found(NOT_FOUND),
        other => V2Error::from(other),
    })
}
