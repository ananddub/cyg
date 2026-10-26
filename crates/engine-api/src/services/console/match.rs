//! `/v2/console/match*` — listing running matches, inspecting one, and terminating
//! one through the match handler's own lifecycle.
//!
//! The registry lives in memory, so none of these routes need the database: a match
//! either is running in this process or it is not. `terminate` goes through
//! `services::match_handler`, never a raw table write.

use axum::Json;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::console::Page;
use crate::dto::console::matches::{ConsoleMatchList, ConsoleMatchRow, ConsoleMatchState};
use crate::error::V2Error;
use crate::services::realtime::RealtimeMatchService;

impl super::ConsoleService {
    /// Page the running matches. The registry is a hash map, so the rows are sorted
    /// by id before slicing to make the paging deterministic across calls. A
    /// scoped request is refused unless this process hosts that game: the registry
    /// belongs to the process, not to a table.
    pub async fn list_matches(
        &self,
        state: &ApiState,
        page: Page,
    ) -> Result<Json<ConsoleMatchList>, V2Error> {
        if let Some(game) = page.game_id {
            crate::services::games::tenant::require_hosted(state, game).await?;
        }
        let mut rows: Vec<ConsoleMatchRow> = RealtimeMatchService::list_snapshots_all()
            .iter()
            .map(ConsoleMatchRow::from)
            .collect();
        rows.sort_by(|a, b| a.api_match.match_id.cmp(&b.api_match.match_id));
        let offset = page.offset() as usize;
        let limit = page.limit() as usize;
        Ok(Json(ConsoleMatchList {
            matches: rows.into_iter().skip(offset).take(limit).collect(),
        }))
    }

    /// Best-effort live state for one match: its joined members. A relay match has
    /// no handler state and no tick counter, so those print as their empty defaults.
    pub async fn match_state(
        &self,
        state: &ApiState,
        game: Option<Uuid>,
        id: &str,
    ) -> Result<Json<ConsoleMatchState>, V2Error> {
        if let Some(game) = game {
            crate::services::games::tenant::require_hosted(state, game).await?;
        }
        let id = match_id(id)?;
        let snapshot = RealtimeMatchService::get_snapshot(id).ok_or_else(|| {
            V2Error::invalid_argument("Match not found, or match handler already stopped.")
        })?;
        Ok(Json(ConsoleMatchState {
            presences: snapshot.members,
            tick: 0,
            state: String::new(),
        }))
    }

    /// Terminate a running match: the handler drops it from the registry and notifies
    /// its members. The audit trail records the operator's action.
    pub async fn terminate_match(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        game: Option<Uuid>,
        id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        if let Some(game) = game {
            crate::services::games::tenant::require_hosted(state, game).await?;
        }
        let id = match_id(id)?;
        if !RealtimeMatchService::terminate_match(id) {
            return Err(V2Error::not_found("Match not found."));
        }
        super::audit(
            state,
            claims,
            "match.terminate",
            "match",
            Some(id),
            &format!("Terminated match {}.", id.simple()),
        )
        .await;
        Ok(Json(Empty {}))
    }
}

/// A match id arrives either bare or as the reference server's `uuid.node` form; the head before
/// the first dot is the registry key either way.
fn match_id(raw: &str) -> Result<Uuid, V2Error> {
    let head = raw.split('.').next().unwrap_or("");
    Uuid::parse_str(head).map_err(|_| V2Error::invalid_argument("Invalid match id."))
}
