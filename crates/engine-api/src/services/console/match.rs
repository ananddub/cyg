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
    pub async fn list_matches(
        &self,
        state: &ApiState,
        page: Page,
    ) -> Result<Json<ConsoleMatchList>, V2Error> {
        if let Some(game) = page.game_id {
            crate::utils::guards::require_hosted(state, game).await?;
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

    pub async fn match_state(
        &self,
        state: &ApiState,
        game: Option<Uuid>,
        id: &str,
    ) -> Result<Json<ConsoleMatchState>, V2Error> {
        if let Some(game) = game {
            crate::utils::guards::require_hosted(state, game).await?;
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

    pub async fn terminate_match(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        game: Option<Uuid>,
        id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        if let Some(game) = game {
            crate::utils::guards::require_hosted(state, game).await?;
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

fn match_id(raw: &str) -> Result<Uuid, V2Error> {
    let head = raw.split('.').next().unwrap_or("");
    Uuid::parse_str(head).map_err(|_| V2Error::invalid_argument("Invalid match id."))
}
