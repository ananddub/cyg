use crate::services::plane::Plane;

use crate::utils::config::configured;

use super::*;

pub async fn resolve(plane: &Plane) -> Result<Option<GameRow>, V2Error> {
    if let Some(raw) = configured(plane.game.game_id.as_deref()) {
        let id = Uuid::parse_str(raw)
            .map_err(|_| V2Error::internal(format!("Configured game_id '{raw}' is not a UUID.")))?;
        let game = by_id(plane, id).await?;
        if game.is_none() {
            tracing::warn!("[tenancy] configured game_id {id} is not registered");
        }
        return Ok(game);
    }
    if let Some(key) = configured(plane.game.game_key.as_deref()) {
        let game = by_key(plane, key).await?;
        if game.is_none() {
            tracing::warn!("[tenancy] configured game_key '{key}' is not registered");
        }
        return Ok(game);
    }
    if let Some(name) = configured(Some(&plane.game.name)) {
        if let Ok(Some(game)) = by_name(plane, name).await {
            return Ok(Some(game));
        }
        match auto_register(plane, name).await {
            Ok(game) => {
                tracing::info!(
                    "[tenancy] auto-registered game '{}' ({})",
                    game.name,
                    game.game_key
                );
                return Ok(Some(game));
            }
            Err(err) => {
                tracing::debug!("[tenancy] auto-registration skipped: {}", err.message);
            }
        }
    }
    sole_active(plane).await
}
pub async fn game_id(plane: &Plane) -> Result<Option<Uuid>, V2Error> {
    Ok(resolve(plane).await?.map(|game| game.id))
}
pub async fn backfill(plane: &Plane, game_id: Uuid) {
    let Ok(queries) = plane.q().await else {
        return;
    };
    for (table, claimed) in [
        ("users", queries.backfill_users_game(game_id).await),
        (
            "leaderboard",
            queries.backfill_leaderboard_game(game_id).await,
        ),
        ("purchase", queries.backfill_purchases_game(game_id).await),
        (
            "subscription",
            queries.backfill_subscriptions_game(game_id).await,
        ),
        (
            "wallet_ledger",
            queries.backfill_wallet_ledger_game(game_id).await,
        ),
    ] {
        match claimed {
            Ok(done) => {
                if done.rows_affected() > 0 {
                    tracing::info!(
                        "[tenancy] claimed {} legacy {table} row(s) for game {game_id}",
                        done.rows_affected()
                    );
                }
            }
            Err(err) => {
                tracing::info!("[tenancy] {table} backfill skipped: {err}");
            }
        }
    }
}
pub async fn stamp_user(plane: &Plane, user_id: Uuid) {
    let Ok(queries) = plane.q().await else {
        return;
    };
    let Ok(Some(game_id)) = game_id(plane).await else {
        return;
    };
    if let Err(err) = queries.stamp_user_game(game_id, user_id).await {
        tracing::warn!("[tenancy] could not stamp user {user_id}: {err}");
    }
}
pub async fn stamp_user_as(plane: &Plane, user_id: Uuid, game_id: Option<Uuid>) {
    let (Ok(queries), Some(game_id)) = (plane.q().await, game_id) else {
        return;
    };
    if let Err(err) = queries.stamp_user_game(game_id, user_id).await {
        tracing::warn!("[tenancy] could not stamp user {user_id}: {err}");
    }
}
pub async fn list_accounts(
    plane: &Plane,
    game: Option<Uuid>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Users>, V2Error> {
    plane
        .q()
        .await?
        .tenancy_accounts(game, limit, offset)
        .await
        .map_err(V2Error::from)
}
