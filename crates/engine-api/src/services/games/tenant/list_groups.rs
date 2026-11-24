use crate::services::plane::Plane;

use super::*;

pub async fn list_groups(
    plane: &Plane,
    game: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Groups>, V2Error> {
    plane
        .q()
        .await?
        .tenancy_groups(game, limit, offset)
        .await
        .map_err(V2Error::from)
}
pub async fn list_boards(
    plane: &Plane,
    game: Option<Uuid>,
    category: i16,
    limit: i64,
    offset: i64,
) -> Result<Vec<Leaderboard>, V2Error> {
    plane
        .q()
        .await?
        .tenancy_boards(game, category, limit, offset)
        .await
        .map_err(V2Error::from)
}
pub async fn list_storage(
    plane: &Plane,
    game: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ListStorageRow>, V2Error> {
    plane
        .q()
        .await?
        .tenancy_storage(game, limit, offset)
        .await
        .map_err(V2Error::from)
}
