//! The two shapes a tournament score write takes in the database.
//!
//! A join-required board only updates the row the join left, so its counter never moves; an
//! open board inserts and takes a place, holding the counter and attempt limit in one txn.

use sqlx::PgPool;

use crate::error::V2Error;
use crate::repositories::models::Leaderboard;
use crate::utils::leaderboard::ascending;
use crate::utils::tournament::statement::{
    EXISTS_SQL, JOIN_SIZE_SQL, Write, accept, accept_joined,
};
use crate::utils::tournament::types::{JOIN_REQUIRED, MAX_ATTEMPTS, MAX_SIZE, WRITE_FAILED};

/// Apply one write to the round, by the way the board admits its entrants.
pub(super) async fn submit(
    db: &PgPool,
    board: &Leaderboard,
    write: &Write<'_>,
) -> Result<(), V2Error> {
    if board.join_required {
        joined(db, board, write).await
    } else {
        entered(db, board, write).await
    }
}

/// The attempt filter rides on the UPDATE's own WHERE, so a caller that has
/// used up its attempts simply changes nothing.
async fn joined(db: &PgPool, board: &Leaderboard, write: &Write<'_>) -> Result<(), V2Error> {
    let exists = sqlx::query(EXISTS_SQL)
        .bind(write.leaderboard_id)
        .bind(write.owner_id)
        .bind(write.expiry)
        .fetch_optional(db)
        .await
        .map_err(|error| {
            eprintln!("[v2] tournament write lookup: {error}");
            V2Error::internal(WRITE_FAILED)
        })?;
    if exists.is_none() {
        return Err(V2Error::failed_precondition(JOIN_REQUIRED));
    }

    sqlx::query(&accept_joined(write, !ascending(board)))
        .bind(write.leaderboard_id)
        .bind(write.owner_id)
        .bind(write.username)
        .bind(write.expiry)
        .bind(write.delta.0)
        .bind(write.delta.1)
        .bind(write.metadata.as_ref())
        .execute(db)
        .await
        .map_err(write_failure)?;
    Ok(())
}

/// The row's attempt counter, read out of the write itself: a fresh insert
/// answers with its own default, and an update with what it moved to.
async fn entered(db: &PgPool, board: &Leaderboard, write: &Write<'_>) -> Result<(), V2Error> {
    let mut tx = db.begin().await.map_err(write_failure)?;
    let counters = sqlx::query_as::<_, (i32, i32)>(&accept(write, !ascending(board)))
        .bind(write.leaderboard_id)
        .bind(write.owner_id)
        .bind(write.username)
        .bind(write.expiry)
        .bind(write.delta.0)
        .bind(write.delta.1)
        .bind(write.metadata.as_ref())
        .bind(write.max_num_score)
        .bind(write.abs.0)
        .bind(write.abs.1)
        .fetch_optional(&mut *tx)
        .await;

    let counters = match counters {
        Ok(Some(counters)) => counters,
        Ok(None) => return commit(tx).await,
        Err(error) if V2Error::is_unique_violation(&error, "leaderboard_record_pkey") => {
            drop(tx);
            return Ok(());
        }
        Err(error) => return Err(write_failure(error)),
    };

    let (num_score, max_num_score) = counters;
    if max_num_score > 0 && i64::from(num_score) > i64::from(max_num_score) {
        return Err(V2Error::failed_precondition(MAX_ATTEMPTS));
    }
    if i64::from(num_score) <= 1 {
        let room = sqlx::query(JOIN_SIZE_SQL)
            .bind(write.leaderboard_id)
            .execute(&mut *tx)
            .await
            .map_err(write_failure)?
            .rows_affected();
        if room == 0 {
            return Err(V2Error::failed_precondition(MAX_SIZE));
        }
    }
    commit(tx).await
}

fn write_failure(error: sqlx::Error) -> V2Error {
    eprintln!("[v2] tournament write: {error}");
    V2Error::internal(WRITE_FAILED)
}

async fn commit(tx: sqlx::Transaction<'_, sqlx::Postgres>) -> Result<(), V2Error> {
    tx.commit().await.map_err(write_failure)
}
