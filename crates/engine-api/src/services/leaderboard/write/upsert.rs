use super::*;

async fn upsert(
    queries: &Queries,
    write: &Write<'_>,
    descending: bool,
) -> Result<LeaderboardRecord, V2Error> {
    let Some(current) = existing(queries, write).await? else {
        return create_record(queries, write).await;
    };
    match next_record(write, &current, descending) {
        Some(row) => update_record(queries, write, &row).await,
        None => read_existing(
            queries,
            GetLeaderboardRecordParams {
                leaderboard_id: write.leaderboard_id.to_string(),
                owner_id: write.owner_id,
                expiry_time: write.expiry,
            },
        )
        .await,
    }
}
pub(super) fn failed(error: DbError) -> V2Error {
    tracing::error!("[v2] leaderboard write: {error}");
    V2Error::internal(WRITE_FAILED)
}
async fn apply(
    queries: &Queries,
    board: &Leaderboard,
    write: &Write<'_>,
) -> Result<RecordResponse, V2Error> {
    let record = upsert(queries, write, !ascending(board)).await?;
    let rank = if ranks_enabled(board) {
        rank_of(queries, board, &record).await.map_err(failed)?
    } else {
        0
    };
    Ok(RecordResponse::from_record(&record, rank))
}
impl super::super::LeaderboardService {
    pub async fn write_record(
        &self,
        state: &ApiState,
        auth: &Authed,
        id: &str,
        body: Value,
    ) -> Result<Json<RecordResponse>, V2Error> {
        if id.is_empty() {
            return Err(V2Error::invalid_argument(INVALID_ID));
        }
        if body.is_null() {
            return Err(V2Error::invalid_argument(BAD_RECORD));
        }
        let input: RecordRequest =
            serde_json::from_value(body).map_err(|_| V2Error::invalid_argument(BAD_RECORD))?;
        let metadata = match input.metadata.as_deref() {
            Some(raw) if !raw.is_empty() => Some(parse_metadata(raw)?),
            _ => None,
        };

        let queries = q(state).await?;
        let board = load_board(&queries, id).await?;
        if board.authoritative {
            return Err(V2Error::permission_denied(AUTH_WRITE));
        }
        let operator = resolve_operator(&board, input.operator)?;
        let username = (!auth.0.un.is_empty()).then_some(auth.0.un.as_str());
        let write = Write {
            leaderboard_id: &board.id,
            owner_id: auth.uid(),
            username,
            abs: (input.score, input.subscore),
            delta: (input.score, input.subscore),
            metadata,
            expiry: record_window(&board)?,
            operator,
        };
        Ok(Json(apply(&queries, &board, &write).await?))
    }
}
