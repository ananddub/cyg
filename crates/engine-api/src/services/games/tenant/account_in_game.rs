use super::*;

pub async fn account_in_game(
    state: &ApiState,
    game: Option<Uuid>,
    user_id: Uuid,
) -> Result<bool, V2Error> {
    let Some(game) = game else {
        return Ok(true);
    };
    q(state).await?
        .tenancy_account_in_game(user_id, game)
        .await
        .map_err(V2Error::from)
}
pub async fn require_account(
    state: &ApiState,
    game: Option<Uuid>,
    user_id: Uuid,
) -> Result<(), V2Error> {
    if account_in_game(state, game, user_id).await? {
        Ok(())
    } else {
        Err(V2Error::not_found("Account not found."))
    }
}
pub async fn require_group(
    state: &ApiState,
    game: Option<Uuid>,
    group_id: Uuid,
) -> Result<(), V2Error> {
    let Some(game) = game else {
        return Ok(());
    };
    if q(state).await?
        .tenancy_group_in_game(group_id, game)
        .await
        .map_err(V2Error::from)?
    {
        Ok(())
    } else {
        Err(V2Error::not_found("Group not found."))
    }
}
pub async fn board_in_game(
    state: &ApiState,
    game: Option<Uuid>,
    board_id: &str,
) -> Result<bool, V2Error> {
    let Some(game) = game else {
        return Ok(true);
    };
    q(state).await?
        .tenancy_board_in_game(board_id, game)
        .await
        .map_err(V2Error::from)
}
pub async fn delete_storage_collection(
    state: &ApiState,
    game: Option<Uuid>,
    collection: &str,
) -> Result<u64, V2Error> {
    q(state).await?
        .tenancy_delete_storage_collection(game, collection)
        .await
        .map_err(V2Error::from)
}
