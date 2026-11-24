use crate::services::plane::Plane;

use super::*;

pub async fn account_in_game(
    plane: &Plane,
    game: Option<Uuid>,
    user_id: Uuid,
) -> Result<bool, V2Error> {
    let Some(game) = game else {
        return Ok(true);
    };
    plane
        .q()
        .await?
        .tenancy_account_in_game(user_id, game)
        .await
        .map_err(V2Error::from)
}
pub async fn board_in_game(
    plane: &Plane,
    game: Option<Uuid>,
    board_id: &str,
) -> Result<bool, V2Error> {
    let Some(game) = game else {
        return Ok(true);
    };
    plane
        .q()
        .await?
        .tenancy_board_in_game(board_id, game)
        .await
        .map_err(V2Error::from)
}
pub async fn delete_storage_collection(
    plane: &Plane,
    game: Option<Uuid>,
    collection: &str,
) -> Result<u64, V2Error> {
    plane
        .q()
        .await?
        .tenancy_delete_storage_collection(game, collection)
        .await
        .map_err(V2Error::from)
}
