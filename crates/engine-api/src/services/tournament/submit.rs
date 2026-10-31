use chrono::Utc;

use crate::db::DbError;
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::repositories::models::{Leaderboard, LeaderboardRecord};
use crate::repositories::tournament::{Attempts, TournamentEntryWrite};
use crate::utils::leaderboard::ascending;
use crate::utils::tournament::statement::{Write, entered_row, next_score};
use crate::utils::tournament::types::{JOIN_REQUIRED, MAX_ATTEMPTS, MAX_SIZE, WRITE_FAILED};

pub(super) async fn submit(
    queries: &Queries,
    board: &Leaderboard,
    write: &Write<'_>,
) -> Result<(), V2Error> {
    let current = lookup(queries, write).await?;
    if board.join_required {
        let Some(current) = current else {
            return Err(V2Error::failed_precondition(JOIN_REQUIRED));
        };
        write_round(queries, board, write, current).await
    } else {
        match current {
            Some(current) => write_round(queries, board, write, current).await,
            None => enter(queries, board, write).await,
        }
    }
}

async fn lookup(
    queries: &Queries,
    write: &Write<'_>,
) -> Result<Option<LeaderboardRecord>, V2Error> {
    queries
        .tournament_entry(write.leaderboard_id, write.owner_id, write.expiry)
        .await
        .map_err(write_failure)
}

async fn write_round(
    queries: &Queries,
    board: &Leaderboard,
    write: &Write<'_>,
    current: LeaderboardRecord,
) -> Result<(), V2Error> {
    if current.max_num_score > 0 && current.num_score >= current.max_num_score {
        return Ok(());
    }
    let (score, subscore) = next_score(write, current.score, current.subscore, !ascending(board));
    let metadata = write
        .metadata
        .clone()
        .unwrap_or_else(|| current.metadata.clone());
    let username = write
        .username
        .map(str::to_string)
        .or_else(|| current.username.clone());

    let counters = queries
        .update_tournament_entry(&TournamentEntryWrite {
            leaderboard_id: write.leaderboard_id,
            owner_id: write.owner_id,
            username: username.as_deref(),
            score,
            subscore,
            metadata,
            expiry: write.expiry,
            now: Utc::now(),
        })
        .await
        .map_err(write_failure)?;
    match counters {
        Some(Attempts {
            num_score,
            max_num_score,
        }) if max_num_score > 0 && i64::from(num_score) > i64::from(max_num_score) => {
            Err(V2Error::failed_precondition(MAX_ATTEMPTS))
        }
        _ => Ok(()),
    }
}

async fn enter(queries: &Queries, board: &Leaderboard, write: &Write<'_>) -> Result<(), V2Error> {
    let room = queries
        .grow_tournament_size(&board.id)
        .await
        .map_err(write_failure)?;
    if room.is_none() {
        return Err(V2Error::failed_precondition(MAX_SIZE));
    }

    let row = entered_row(write, Utc::now());
    match queries.create_leaderboard_record(&row).await {
        Ok(stored) => {
            if stored.max_num_score > 0 && stored.num_score > stored.max_num_score {
                return Err(V2Error::failed_precondition(MAX_ATTEMPTS));
            }
            Ok(())
        }
        Err(DbError::Unique(index)) if index == "leaderboard_record_pkey" => {
            release(queries, board).await;
            Ok(())
        }
        Err(error) => {
            release(queries, board).await;
            Err(write_failure(error))
        }
    }
}

async fn release(queries: &Queries, board: &Leaderboard) {
    if let Err(error) = queries.shrink_tournament_size(&board.id).await {
        tracing::error!("[v2] tournament write: releasing a place: {error}");
    }
}

fn write_failure(error: DbError) -> V2Error {
    tracing::error!("[v2] tournament write: {error}");
    V2Error::internal(WRITE_FAILED)
}
