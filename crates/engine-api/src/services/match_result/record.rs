use std::collections::{BTreeMap, HashMap};

use axum::Json;
use uuid::Uuid;

use super::{MAX_COUNTERS, MAX_INCREMENT, MAX_PARTICIPANTS, MAX_SCORE, MatchResultService};
use crate::ApiState;
use crate::db::DbError;
use crate::dto::match_result::{MatchResultRequest, MatchResultResponse, RecordedScore};
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::repositories::match_result::{CounterWrite, ScoreWrite};
use crate::repositories::models::Leaderboard;
use crate::services::deps::q;
use crate::services::quest::catalog::counter_name;
use crate::services::realtime::RealtimeMatchService;
use crate::utils::leaderboard::types::{BAD_METADATA, NOT_FOUND, ascending};
use crate::utils::leaderboard::window::record_window;

impl MatchResultService {
    /// What the host says happened, credited only to the players the control
    /// plane seated in that match. A score is never a player's own claim: this is
    /// the door the server key opens.
    pub async fn record_result(
        &self,
        state: &ApiState,
        raw_id: &str,
        req: MatchResultRequest,
    ) -> Result<Json<MatchResultResponse>, V2Error> {
        let match_id = Uuid::parse_str(raw_id).map_err(|_| not_a_match())?;
        let seats = seated(match_id).ok_or_else(not_a_match)?;
        let counters = counter_writes(&req)?;
        let queries = q(state).await?;
        let board = match req.leaderboard.as_deref().filter(|name| !name.trim().is_empty()) {
            None => None,
            Some(name) => Some(board_of(&queries, name).await?),
        };
        check(&req, &seats)?;
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

/// The players the control plane seated in this match, with the name each one
/// held on the socket, which is what a leaderboard record shows.
fn seated(match_id: Uuid) -> Option<HashMap<Uuid, String>> {
    let snapshot = RealtimeMatchService::get_snapshot(match_id)?;
    let mut seats = HashMap::with_capacity(snapshot.members.len());
    for member in snapshot.members {
        if let Ok(uid) = Uuid::parse_str(&member.user_id) {
            seats.insert(uid, member.username);
        }
    }
    Some(seats)
}

/// Every named player has to be one that was seated, and seated once.
fn check(req: &MatchResultRequest, seats: &HashMap<Uuid, String>) -> Result<(), V2Error> {
    if req.participants.is_empty() || req.participants.len() > MAX_PARTICIPANTS {
        return Err(V2Error::invalid_argument(format!(
            "A match result names between 1 and {MAX_PARTICIPANTS} players."
        )));
    }
    let mut named: Vec<Uuid> = Vec::with_capacity(req.participants.len());
    for part in &req.participants {
        if named.contains(&part.user_id) {
            return Err(V2Error::invalid_argument(
                "A match result names each player once.",
            ));
        }
        if !seats.contains_key(&part.user_id) {
            return Err(V2Error::invalid_argument(
                "A match result can only name a player who was in the match.",
            ));
        }
        if !(part.score.abs() <= MAX_SCORE && part.subscore.abs() <= MAX_SCORE) {
            return Err(V2Error::invalid_argument(format!(
                "A match score is between -{MAX_SCORE} and {MAX_SCORE}."
            )));
        }
        if part
            .metadata
            .as_ref()
            .is_some_and(|value| !value.is_object())
        {
            return Err(V2Error::invalid_argument(BAD_METADATA));
        }
        named.push(part.user_id);
    }
    Ok(())
}

fn counter_writes(req: &MatchResultRequest) -> Result<Vec<CounterWrite>, V2Error> {
    let mut writes = Vec::new();
    for part in &req.participants {
        for (name, delta) in &part.stats {
            let stat = counter_name(name)?.to_string();
            if !(1..=MAX_INCREMENT).contains(delta) {
                return Err(V2Error::invalid_argument(format!(
                    "The {stat} increment must be between 1 and {MAX_INCREMENT}."
                )));
            }
            writes.push(CounterWrite {
                user_id: part.user_id,
                stat,
                delta: *delta,
            });
        }
    }
    if writes.len() > MAX_COUNTERS {
        return Err(V2Error::invalid_argument(format!(
            "A match result raises at most {MAX_COUNTERS} counters."
        )));
    }
    Ok(writes)
}

async fn board_of(queries: &Queries, name: &str) -> Result<Leaderboard, V2Error> {
    queries
        .get_leaderboard(name.trim())
        .await
        .map_err(|_| V2Error::not_found(NOT_FOUND))
}

/// Counters and scores in one transaction, retried while another writer keeps
/// clashing with us. An aborted transaction raised nothing, so a retry cannot
/// double-count.
async fn write(
    queries: &Queries,
    counters: &[CounterWrite],
    req: &MatchResultRequest,
    board: Option<&Leaderboard>,
    seats: &HashMap<Uuid, String>,
) -> Result<(), V2Error> {
    for attempt in 0..3 {
        let scores = score_writes(queries, req, board, seats).await?;
        match queries.record_match_outcome(counters, &scores).await {
            Ok(()) => return Ok(()),
            Err(DbError::Unique(_)) if attempt < 2 => continue,
            Err(DbError::Unique(_)) => Err(V2Error::failed_precondition(
                "This result could not be posted while another write was in flight.",
            ))?,
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}

async fn score_writes(
    queries: &Queries,
    req: &MatchResultRequest,
    board: Option<&Leaderboard>,
    seats: &HashMap<Uuid, String>,
) -> Result<Vec<ScoreWrite>, V2Error> {
    let Some(board) = board else {
        return Ok(Vec::new());
    };
    let expiry = record_window(board)?;
    let descending = !ascending(board);
    let mut scores = Vec::with_capacity(req.participants.len());
    for part in &req.participants {
        scores.push(ScoreWrite {
            leaderboard_id: board.id.clone(),
            owner_id: part.user_id,
            expiry,
            score: part.score,
            subscore: part.subscore,
            username: seats.get(&part.user_id).cloned(),
            metadata: part.metadata.clone(),
            operator: board.operator,
            descending,
            present: queries
                .leaderboard_record_for_write(&board.id, part.user_id, expiry)
                .await?
                .is_some(),
            max_num_score: board.max_num_score,
        });
    }
    Ok(scores)
}

/// What the result left behind, read back rather than echoed: a board that keeps
/// the best score answers with the score it holds.
async fn read_back(
    queries: &Queries,
    board: Option<&Leaderboard>,
    req: &MatchResultRequest,
) -> Result<Vec<RecordedScore>, V2Error> {
    let expiry = board.map(record_window).transpose()?;
    let mut recorded = Vec::with_capacity(req.participants.len());
    for part in &req.participants {
        let mut counters = BTreeMap::new();
        for name in part.stats.keys() {
            let stat = counter_name(name)?;
            if let Some(row) = queries.get_player_stat(part.user_id, stat).await? {
                counters.insert(stat.to_string(), row.value);
            }
        }
        let on_board = match (&board, expiry) {
            (Some(board), Some(expiry)) => {
                queries
                    .leaderboard_record_for_write(&board.id, part.user_id, expiry)
                    .await?
            }
            _ => None,
        };
        recorded.push(RecordedScore {
            user_id: part.user_id,
            score: on_board.as_ref().map_or(0, |row| row.score),
            subscore: on_board.as_ref().map_or(0, |row| row.subscore),
            counters,
        });
    }
    Ok(recorded)
}
