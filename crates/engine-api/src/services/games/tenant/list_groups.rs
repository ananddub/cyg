use super::*;

pub async fn list_groups(
    state: &ApiState,
    game: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Groups>, V2Error> {
    q(state).await?
        .tenancy_groups(game, limit, offset)
        .await
        .map_err(V2Error::from)
}
pub async fn list_boards(
    state: &ApiState,
    game: Option<Uuid>,
    category: i16,
    limit: i64,
    offset: i64,
) -> Result<Vec<Leaderboard>, V2Error> {
    q(state).await?
        .tenancy_boards(game, category, limit, offset)
        .await
        .map_err(V2Error::from)
}
pub async fn list_storage(
    state: &ApiState,
    game: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ListStorageRow>, V2Error> {
    q(state).await?
        .tenancy_storage(game, limit, offset)
        .await
        .map_err(V2Error::from)
}
