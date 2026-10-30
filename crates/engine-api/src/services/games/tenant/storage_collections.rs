use crate::services::deps::q;

use super::*;

pub async fn storage_collections(
    state: &ApiState,
    game: Option<Uuid>,
) -> Result<Vec<String>, V2Error> {
    q(state)
        .await?
        .tenancy_storage_collections(game)
        .await
        .map_err(V2Error::from)
}
pub async fn storage_in_collection(
    state: &ApiState,
    game: Option<Uuid>,
    collection: &str,
    limit: i64,
) -> Result<Vec<ListStorageRow>, V2Error> {
    q(state)
        .await?
        .tenancy_storage_in_collection(game, collection, limit)
        .await
        .map_err(V2Error::from)
}
pub async fn list_purchases(
    state: &ApiState,
    game: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Purchase>, V2Error> {
    q(state)
        .await?
        .tenancy_purchases(game, limit, offset)
        .await
        .map_err(V2Error::from)
}
pub async fn list_subscriptions(
    state: &ApiState,
    game: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Subscription>, V2Error> {
    q(state)
        .await?
        .tenancy_subscriptions(game, limit, offset)
        .await
        .map_err(V2Error::from)
}
