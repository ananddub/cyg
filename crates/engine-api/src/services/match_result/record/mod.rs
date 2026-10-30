mod check;
mod outcome;

use axum::Json;
use uuid::Uuid;

use crate::ApiState;
use crate::dto::match_result::{MatchResultRequest, MatchResultResponse};
use crate::error::V2Error;
use crate::services::deps::q;
use crate::services::match_result::MatchResultService;

use check::{check, counter_writes, seated};
use outcome::{board_of, read_back, write};

impl MatchResultService {
    /// What the host says happened, credited only to the players the control
    /// plane seated in that match. A score is never a player's own claim: this is
    /// the door the server key opens.
    pub async fn record_result(
        &self,
        state: &ApiState,
        raw_id: &str,
        body: serde_json::Value,
    ) -> Result<Json<MatchResultResponse>, V2Error> {
        let req: MatchResultRequest = serde_json::from_value(body).map_err(|_| {
            V2Error::invalid_argument(
                "A match result needs participants, each with a user id and a score.",
            )
        })?;
        let match_id = Uuid::parse_str(raw_id).map_err(|_| not_a_match())?;
        let seats = seated(match_id).ok_or_else(not_a_match)?;
        check(&req, &seats)?;
        let counters = counter_writes(&req)?;
        let queries = q(state).await?;
        let board = match req
            .leaderboard
            .as_deref()
            .filter(|name| !name.trim().is_empty())
        {
            None => None,
            Some(name) => Some(board_of(&queries, name).await?),
        };
        write(&queries, &counters, &req, board.as_ref(), &seats).await?;
        let recorded = read_back(&queries, board.as_ref(), &req).await?;
        crate::services::webhook::emit(
            state,
            "match.ended",
            &match_id.simple().to_string(),
            serde_json::json!({
                "match_id": match_id,
                "leaderboard": board.as_ref().map(|board| board.id.clone()),
                "participants": recorded,
            }),
        )
        .await;
        Ok(Json(MatchResultResponse {
            success: true,
            match_id,
            leaderboard: board.map(|board| board.id),
            recorded,
        }))
    }
}

fn not_a_match() -> V2Error {
    V2Error::not_found("Match not found.")
}
