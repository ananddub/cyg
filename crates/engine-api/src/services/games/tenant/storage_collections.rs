use crate::services::plane::Plane;

use super::*;

pub async fn storage_collections(
    plane: &Plane,
    game: Option<Uuid>,
) -> Result<Vec<String>, V2Error> {
    plane
        .q()
        .await?
        .tenancy_storage_collections(game)
        .await
        .map_err(V2Error::from)
}
pub async fn storage_in_collection(
    plane: &Plane,
    game: Option<Uuid>,
    collection: &str,
    limit: i64,
) -> Result<Vec<ListStorageRow>, V2Error> {
    plane
        .q()
        .await?
        .tenancy_storage_in_collection(game, collection, limit)
        .await
        .map_err(V2Error::from)
}
pub async fn list_purchases(
    plane: &Plane,
    game: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Purchase>, V2Error> {
    plane
        .q()
        .await?
        .tenancy_purchases(game, limit, offset)
        .await
        .map_err(V2Error::from)
}
pub async fn list_subscriptions(
    plane: &Plane,
    game: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Subscription>, V2Error> {
    plane
        .q()
        .await?
        .tenancy_subscriptions(game, limit, offset)
        .await
        .map_err(V2Error::from)
}
