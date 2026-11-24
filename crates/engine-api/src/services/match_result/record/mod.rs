mod check;
mod outcome;

use axum::Json;
use uuid::Uuid;

use crate::dto::match_result::{MatchResultRequest, MatchResultResponse};
use crate::error::V2Error;
use crate::services::match_result::MatchResultService;

use check::{check, counter_writes, seated};
use outcome::{board_of, read_back, write};

impl MatchResultService {
    /// What the host says happened, credited only to the players the control
    /// plane seated in that match. A score is never a player's own claim: this is
    /// the door the server key opens.
    pub async fn record_result(
        &self,

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
        self.apply(match_id, req, seats).await
    }

    /// Store a result the caller has already attributed to a set of players.
    ///
    /// A room's own report arrives with the players it seated, resolved by the
    /// node that held them — the realtime registry has no record of a room that
    /// has already closed. Everything after the seating check is shared with the
    /// REST path, so a room cannot store a result the endpoint would refuse.
    pub async fn apply(
        &self,

        match_id: Uuid,
        req: MatchResultRequest,
        seats: std::collections::HashMap<Uuid, String>,
    ) -> Result<Json<MatchResultResponse>, V2Error> {
        check(&req, &seats)?;
        let counters = counter_writes(&req)?;
        let queries = self.plane.q().await?;
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
        // Once per participant, after the result is stored: a rule bound to
        // match_end wants to reward a player for a match that actually counted,
        // and a match that failed to record has nothing to reward.
        for part in &req.participants {
            crate::services::rules::dispatch::fire(&self.plane, "match_end", part.user_id).await;
        }
        crate::services::webhook::emit(
            &self.plane,
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
