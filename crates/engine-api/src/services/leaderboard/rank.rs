use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::db::DbError;
use crate::dto::leaderboard::RecordResponse;
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::repositories::leaderboard::GetLeaderboardRecordRankDescParams;
use crate::repositories::models::{Leaderboard, LeaderboardRecord};
use crate::utils::leaderboard::types::{ascending, ranks_enabled};
use crate::utils::leaderboard::wire::int64;

pub(super) async fn rank(
    queries: &Queries,
    board: &Leaderboard,
    window: DateTime<Utc>,
    score: i64,
    subscore: i64,
    owner_id: Uuid,
) -> Result<i64, DbError> {
    if !ranks_enabled(board) {
        return Ok(0);
    }
    let params = GetLeaderboardRecordRankDescParams {
        leaderboard_id: board.id.clone(),
        expiry_time: window,
        score,
        subscore,
        owner_id,
    };
    let better = if ascending(board) {
        queries.get_leaderboard_record_rank_asc(&params).await?
    } else {
        queries.get_leaderboard_record_rank_desc(&params).await?
    };
    Ok(better + 1)
}

pub(crate) async fn rank_of(
    queries: &Queries,
    board: &Leaderboard,
    record: &LeaderboardRecord,
) -> Result<i64, DbError> {
    rank(
        queries,
        board,
        record.expiry_time,
        record.score,
        record.subscore,
        record.owner_id,
    )
    .await
}

pub(super) async fn ranks(
    queries: &Queries,
    board: &Leaderboard,
    rows: &[LeaderboardRecord],
) -> Result<Vec<i64>, DbError> {
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(rank_of(queries, board, row).await?);
    }
    Ok(out)
}

pub(super) async fn wires(
    queries: &Queries,
    board: &Leaderboard,
    rows: &[LeaderboardRecord],
) -> Result<Vec<RecordResponse>, V2Error> {
    let ranks = ranks(queries, board, rows).await.map_err(V2Error::from)?;
    Ok(rows
        .iter()
        .zip(ranks)
        .map(|(row, rank)| RecordResponse::from_record(row, rank))
        .collect())
}

pub(crate) async fn rank_count(
    queries: &Queries,
    board: &Leaderboard,
    window: DateTime<Utc>,
) -> Result<Option<String>, V2Error> {
    if !ranks_enabled(board) {
        return Ok(None);
    }
    let count = queries
        .count_leaderboard_records(&board.id, window)
        .await
        .map_err(V2Error::from)?;
    Ok(int64(count))
}
