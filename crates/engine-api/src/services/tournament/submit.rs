use std::collections::BTreeMap;

use chrono::Utc;
use serde_json::{Value, json};

use crate::db::DbError;
use crate::error::V2Error;
use crate::repositories::db::{Queries, params, ts};
use crate::repositories::models::{Leaderboard, LeaderboardRecord};
use crate::utils::leaderboard::ascending;
use crate::utils::tournament::statement::{
    Attempts, EXISTS_SQL, JOIN_SIZE_SQL, RELEASE_SIZE_SQL, UPDATE_SQL, Write, entered_row,
    next_score,
};
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

fn tuple_params(write: &Write<'_>) -> BTreeMap<String, Value> {
    params!(
        leaderboard_id = write.leaderboard_id,
        owner_id = write.owner_id.to_string(),
        expiry_time = ts(write.expiry),
    )
}

async fn lookup(
    queries: &Queries,
    write: &Write<'_>,
) -> Result<Option<LeaderboardRecord>, V2Error> {
    queries
        .opt(EXISTS_SQL, tuple_params(write))
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

    let mut binds = tuple_params(write);
    binds.insert("score".to_string(), json!(score));
    binds.insert("subscore".to_string(), json!(subscore));
    binds.insert("metadata".to_string(), json!(metadata));
    binds.insert("username".to_string(), json!(username));
    binds.insert("update_time".to_string(), ts(Utc::now()));

    let counters: Option<Attempts> = queries
        .opt(UPDATE_SQL, binds)
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
    let room: Option<Leaderboard> = queries
        .opt(JOIN_SIZE_SQL, params!(leaderboard_id = &board.id))
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
    if let Err(error) = queries
        .exec(RELEASE_SIZE_SQL, params!(leaderboard_id = &board.id))
        .await
    {
        tracing::error!("[v2] tournament write: releasing a place: {error}");
    }
}

fn write_failure(error: DbError) -> V2Error {
    tracing::error!("[v2] tournament write: {error}");
    V2Error::internal(WRITE_FAILED)
}
