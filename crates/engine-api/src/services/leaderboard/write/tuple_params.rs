use super::*;

const DEFAULT_MAX_NUM_SCORE: i32 = 1_000_000;

pub(super) async fn existing(
    queries: &Queries,
    write: &Write<'_>,
) -> Result<Option<LeaderboardRecord>, V2Error> {
    queries
        .leaderboard_record_for_write(write.leaderboard_id, write.owner_id, write.expiry)
        .await
        .map_err(failed)
}
pub(super) async fn read_existing(
    queries: &Queries,
    params: GetLeaderboardRecordParams,
) -> Result<LeaderboardRecord, V2Error> {
    queries
        .get_leaderboard_record(&params)
        .await
        .map_err(|error| {
            tracing::error!("[v2] leaderboard write read-back: {error}");
            V2Error::internal(WRITE_FAILED)
        })
}
pub(super) async fn create_record(
    queries: &Queries,
    write: &Write<'_>,
) -> Result<LeaderboardRecord, V2Error> {
    let now = Utc::now();
    let zeroed = write.operator == DB_DECREMENT;
    let row = LeaderboardRecord {
        leaderboard_id: write.leaderboard_id.to_string(),
        owner_id: write.owner_id,
        username: write.username.map(str::to_string),
        score: if zeroed { 0 } else { write.abs.0 },
        subscore: if zeroed { 0 } else { write.abs.1 },
        num_score: 1,
        metadata: write.metadata.clone().unwrap_or_else(|| json!({})),
        create_time: now,
        update_time: now,
        expiry_time: write.expiry,
        max_num_score: DEFAULT_MAX_NUM_SCORE,
    };
    match queries.create_leaderboard_record(&row).await {
        Ok(record) => Ok(record),
        Err(DbError::Unique(index)) if index == "leaderboard_record_pkey" => {
            existing(queries, write).await?.ok_or_else(|| {
                tracing::error!("[v2] leaderboard write: unique clash but no record");
                V2Error::internal(WRITE_FAILED)
            })
        }
        Err(error) => Err(failed(error)),
    }
}
pub(super) async fn update_record(
    queries: &Queries,
    write: &Write<'_>,
    row: &LeaderboardRecord,
) -> Result<LeaderboardRecord, V2Error> {
    match queries
        .update_leaderboard_record(row)
        .await
        .map_err(failed)?
    {
        Some(record) => Ok(record),
        None => create_record(queries, write).await,
    }
}
pub(super) fn next_record(
    write: &Write<'_>,
    current: &LeaderboardRecord,
    descending: bool,
) -> Option<LeaderboardRecord> {
    let (abs_score, abs_subscore) = write.abs;
    let (delta_score, delta_subscore) = write.delta;
    let changed = match write.operator {
        DB_SET => current.score != abs_score || current.subscore != abs_subscore,
        DB_INCREMENT | DB_DECREMENT => delta_score != 0 || delta_subscore != 0,
        _ if descending => current.score < abs_score || current.subscore < abs_subscore,
        _ => current.score > abs_score || current.subscore > abs_subscore,
    };
    if !changed {
        return None;
    }

    let mut row = current.clone();
    match write.operator {
        DB_SET => {
            row.score = abs_score;
            row.subscore = abs_subscore;
        }
        DB_INCREMENT => {
            row.score += delta_score;
            row.subscore += delta_subscore;
        }
        DB_DECREMENT => {
            row.score = (row.score - abs_score).max(0);
            row.subscore = (row.subscore - abs_subscore).max(0);
        }
        _ if descending => {
            row.score = row.score.max(abs_score);
            row.subscore = row.subscore.max(abs_subscore);
        }
        _ => {
            row.score = row.score.min(abs_score);
            row.subscore = row.subscore.min(abs_subscore);
        }
    }
    row.num_score += 1;
    if let Some(metadata) = &write.metadata {
        row.metadata = metadata.clone();
    }
    if let Some(username) = write.username {
        row.username = Some(username.to_string());
    }
    row.update_time = Utc::now();
    Some(row)
}
