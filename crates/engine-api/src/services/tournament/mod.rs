use crate::services::plane::Plane;
use auto_di::singleton;
use std::sync::Arc;

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

pub struct TournamentService {
    pub(crate) plane: Arc<Plane>,
}

#[singleton]
impl TournamentService {
    pub fn new(plane: Arc<Plane>) -> Self {
        Self { plane }
    }
}

/// Fails closed when a tournament names a rule: the rule must be published and
/// the player's current stat map must satisfy it. A board without a rule is
/// always open, so existing tournaments are unchanged.
pub(crate) async fn require_rule(
    queries: &Queries,
    board: &Leaderboard,
    user_id: uuid::Uuid,
) -> Result<(), V2Error> {
    let Some(key) = board.rule_key.as_deref().filter(|key| !key.is_empty()) else {
        return Ok(());
    };
    let rule = queries.active_rule(key).await.map_err(|error| {
        tracing::error!("[v2] tournament rule load: {error}");
        V2Error::internal(crate::utils::tournament::types::RULE_INVALID)
    })?;
    let Some(rule) = rule else {
        return Err(V2Error::failed_precondition(
            crate::utils::tournament::types::RULE_UNAVAILABLE,
        ));
    };
    let context = queries
        .player_stat_context(user_id)
        .await
        .map_err(|error| {
            tracing::error!("[v2] tournament rule context: {error}");
            V2Error::internal(crate::utils::tournament::types::RULE_INVALID)
        })?;
    match crate::services::rules::evaluate(&rule.body, &context) {
        Ok(true) => Ok(()),
        Ok(false) => Err(V2Error::failed_precondition(
            crate::utils::tournament::types::RULE_NOT_MET,
        )),
        Err(error) => {
            tracing::error!("[v2] tournament rule '{key}' does not evaluate: {error}");
            Err(V2Error::failed_precondition(
                crate::utils::tournament::types::RULE_INVALID,
            ))
        }
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
