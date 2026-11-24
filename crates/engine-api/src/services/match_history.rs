use crate::services::plane::Plane;
use auto_di::singleton;
use axum::Json;
use std::sync::Arc;

use crate::dto::match_history::ReplayBundle;
use crate::error::V2Error;
use crate::repositories::models::{DesyncReport, GameplayEvent, MatchHistory};

/// Default rows per read when the caller asks for no limit.
const DEFAULT_PAGE: i64 = 200;

/// Mirrored match history: what a room's game emitted, and where a client's
/// simulation stopped agreeing with the host.
pub struct MatchHistoryService {
    pub(crate) plane: Arc<Plane>,
}

#[singleton]
impl MatchHistoryService {
    pub fn new(plane: Arc<Plane>) -> Self {
        Self { plane }
    }
}

impl MatchHistoryService {
    fn page(limit: Option<i64>) -> i64 {
        limit.unwrap_or(DEFAULT_PAGE)
    }

    /// One room's event history, newest first. A room's snapshot is already
    /// readable by any signed-in player (`GET /v1/match`), so this discloses no
    /// more than the match the caller can already see.
    pub async fn events(
        &self,

        room: &str,
        limit: Option<i64>,
    ) -> Result<Json<Vec<GameplayEvent>>, V2Error> {
        let queries = self.plane.q().await?;
        Ok(Json(
            queries
                .list_gameplay_events(room, Self::page(limit))
                .await?,
        ))
    }

    /// Desync evidence, newest first, optionally one room.
    pub async fn desyncs(
        &self,

        room: Option<&str>,
        limit: Option<i64>,
    ) -> Result<Json<Vec<DesyncReport>>, V2Error> {
        let queries = self.plane.q().await?;
        Ok(Json(
            queries.list_desync_reports(room, Self::page(limit)).await?,
        ))
    }

    /// One room's finished matches, newest first. Same disclosure level as
    /// `GET /v1/match`, which already serves a room's snapshot to any signed-in
    /// player: nothing here reaches a caller that could not already read the
    /// live room.
    pub async fn history(
        &self,

        room: &str,
        limit: Option<i64>,
    ) -> Result<Json<Vec<MatchHistory>>, V2Error> {
        let queries = self.plane.q().await?;
        Ok(Json(
            queries.list_match_history(room, Self::page(limit)).await?,
        ))
    }

    /// Everything a replay needs: the last snapshot the host saved (the state a
    /// re-simulation starts from) and the event rows that follow it. The room may
    /// have been disposed an hour ago — a missing snapshot is a 200 with an
    /// absent field, not a 404, because the events are still the answer.
    pub async fn replay(&self, room: &str) -> Result<Json<ReplayBundle>, V2Error> {
        let queries = self.plane.q().await?;
        let snapshot = queries.load_room_snapshot(room).await?;
        Ok(Json(ReplayBundle {
            room: room.to_string(),
            backend: snapshot.as_ref().map(|row| row.backend.clone()),
            snapshot_seq: snapshot.as_ref().map(|row| row.snapshot_seq),
            snapshot: snapshot.as_ref().map(|row| row.snapshot_bytes.clone()),
            matches: queries.list_match_history(room, DEFAULT_PAGE).await?,
            events: queries.list_gameplay_events(room, DEFAULT_PAGE).await?,
        }))
    }
}
