use super::*;

pub async fn delete_notification(
    state: &ApiState,
    game: Option<Uuid>,
    id: Uuid,
) -> Result<bool, V2Error> {
    Ok(q(state).await?
        .tenancy_delete_notification(game, id)
        .await
        .map_err(V2Error::from)?
        > 0)
}
pub async fn delete_subscriptions_for_product(
    state: &ApiState,
    game: Option<Uuid>,
    product_id: &str,
) -> Result<u64, V2Error> {
    q(state).await?
        .tenancy_delete_subscriptions_for_product(game, product_id)
        .await
        .map_err(V2Error::from)
}
pub(super) fn configured(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}
pub(super) async fn by_id(state: &ApiState, id: Uuid) -> Result<Option<GameRow>, V2Error> {
    Ok(q(state).await?
        .tenancy_game_by_id(id)
        .await
        .map_err(V2Error::from)?
        .into_iter()
        .next()
        .map(GameRow::from))
}
pub(super) async fn by_key(state: &ApiState, key: &str) -> Result<Option<GameRow>, V2Error> {
    Ok(q(state).await?
        .tenancy_game_by_key(key)
        .await
        .map_err(V2Error::from)?
        .into_iter()
        .next()
        .map(GameRow::from))
}
pub(super) async fn by_name(state: &ApiState, name: &str) -> Result<Option<GameRow>, V2Error> {
    Ok(q(state).await?
        .tenancy_game_by_name_or_key(name)
        .await
        .map_err(V2Error::from)?
        .map(GameRow::from))
}
pub(super) async fn auto_register(state: &ApiState, name: &str) -> Result<GameRow, V2Error> {
    let dto = crate::services::games::GamesService::create_game(
        state,
        &crate::dto::console::games::CreateGameRequest {
            name: name.to_string(),
            engine_type: Some("custom".to_string()),
            tick_rate: Some(state.game.tick_rate),
            max_clients: Some(state.game.max_clients.max(1) as u32),
            persistent_rooms: None,
            auto_close_seconds: None,
            config: None,
        },
    )
    .await?;
    by_id(state, dto.id)
        .await?
        .ok_or_else(|| V2Error::internal("Auto-registered game could not be re-read"))
}
pub(super) async fn sole_active(state: &ApiState) -> Result<Option<GameRow>, V2Error> {
    let mut rows = q(state).await?
        .tenancy_sole_active_games()
        .await
        .map_err(V2Error::from)?;
    if rows.len() == 1 {
        Ok(rows.pop().map(GameRow::from))
    } else {
        Ok(None)
    }
}
