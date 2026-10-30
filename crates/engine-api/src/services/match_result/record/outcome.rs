use std::collections::{BTreeMap, HashMap};

use uuid::Uuid;

use crate::db::DbError;
use crate::dto::match_result::{MatchResultRequest, ParticipantResult, RecordedScore};
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::repositories::match_result::{CounterWrite, ScoreWrite};
use crate::repositories::models::Leaderboard;
use crate::services::quest::catalog::counter_name;
use crate::utils::leaderboard::types::{NOT_FOUND, ascending};
use crate::utils::leaderboard::window::record_window;

pub(super) async fn board_of(queries: &Queries, name: &str) -> Result<Leaderboard, V2Error> {
    queries
        .get_leaderboard(name.trim())
        .await
        .map_err(|_| V2Error::not_found(NOT_FOUND))
}

/// Counters and scores in one transaction, retried while another writer keeps
/// clashing with us. An aborted transaction raised nothing, so a retry cannot
/// double-count.
pub(super) async fn write(
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
            Err(DbError::Unique(_)) => {
                return Err(V2Error::failed_precondition(
                    "This result could not be posted while another write was in flight.",
                ));
            }
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
pub(super) async fn read_back(
    queries: &Queries,
    board: Option<&Leaderboard>,
    req: &MatchResultRequest,
) -> Result<Vec<RecordedScore>, V2Error> {
    let expiry = board.map(record_window).transpose()?;
    let mut recorded = Vec::with_capacity(req.participants.len());
    for part in &req.participants {
        let counters = counters_now(queries, part).await?;
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

async fn counters_now(
    queries: &Queries,
    part: &ParticipantResult,
) -> Result<BTreeMap<String, i64>, V2Error> {
    let mut counters = BTreeMap::new();
    for name in part.stats.keys() {
        let stat = counter_name(name)?;
        if let Some(row) = queries.get_player_stat(part.user_id, stat).await? {
            counters.insert(stat.to_string(), row.value);
        }
    }
    Ok(counters)
}
