use auto_di::singleton;
use axum::Json;

use crate::ApiState;
use crate::error::V2Error;
use crate::repositories::models::{DesyncReport, GameplayEvent};
use crate::services::deps::q;

/// Default rows per read when the caller asks for no limit.
const DEFAULT_PAGE: i64 = 200;

/// Mirrored match history: what a room's game emitted, and where a client's
/// simulation stopped agreeing with the host.
pub struct MatchHistoryService;

#[singleton]
impl MatchHistoryService {
    fn new() -> Self {
        Self
    }
}

impl MatchHistoryService {
    fn page(limit: Option<i64>) -> i64 {
        limit.unwrap_or(DEFAULT_PAGE)
    }

    /// One room's event history, newest first. A room's snapshot is already
    /// readable by any signed-in player (`GET /v2/match`), so this discloses no
    /// more than the match the caller can already see.
    pub async fn events(
        &self,
        state: &ApiState,
        room: &str,
        limit: Option<i64>,
    ) -> Result<Json<Vec<GameplayEvent>>, V2Error> {
        let queries = q(state).await?;
        Ok(Json(
            queries
                .list_gameplay_events(room, Self::page(limit))
                .await?,
        ))
    }

    /// Desync evidence, newest first, optionally one room.
    pub async fn desyncs(
        &self,
        state: &ApiState,
        room: Option<&str>,
        limit: Option<i64>,
    ) -> Result<Json<Vec<DesyncReport>>, V2Error> {
        let queries = q(state).await?;
        Ok(Json(
            queries
                .list_desync_reports(room, Self::page(limit))
                .await?,
        ))
    }
}
