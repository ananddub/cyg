use uuid::Uuid;

use crate::ApiState;
use crate::db::DbError;
use crate::error::V2Error;
use crate::repositories::db::Queries;

pub async fn require_account(
    state: &ApiState,
    game: Option<Uuid>,
    user_id: Uuid,
) -> Result<(), V2Error> {
    if crate::services::games::tenant::account_in_game(state, game, user_id).await? {
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
    let in_game = crate::services::deps::q(state)
        .await?
        .tenancy_group_in_game(group_id, game)
        .await
        .map_err(V2Error::from)?;
    if in_game {
        Ok(())
    } else {
        Err(V2Error::not_found("Group not found."))
    }
}

pub async fn require_hosted(state: &ApiState, claimed: Uuid) -> Result<(), V2Error> {
    let _ = crate::services::games::GamesService::find_game(state, claimed).await?;
    Ok(())
}

pub async fn require_role(
    queries: &Queries,
    uid: Uuid,
    gid: Uuid,
    max_role: i16,
) -> Result<i16, V2Error> {
    let role = queries
        .get_group_edge_state(uid, gid)
        .await
        .map_err(|e| match e {
            DbError::NotFound => V2Error::not_found("Group not found or permission denied."),
            other => V2Error::from(other),
        })?;
    if role > max_role {
        return Err(V2Error::not_found("Group not found or permission denied."));
    }
    Ok(role)
}
