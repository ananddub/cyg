use std::sync::Arc;

use crate::services::plane::Plane;

use uuid::Uuid;

use crate::db::DbError;
use crate::error::V2Error;
use crate::repositories::db::Queries;

pub async fn require_account(
    plane: &Plane,
    game: Option<Uuid>,
    user_id: Uuid,
) -> Result<(), V2Error> {
    if crate::services::games::tenant::account_in_game(plane, game, user_id).await? {
        Ok(())
    } else {
        Err(V2Error::not_found("Account not found."))
    }
}

pub async fn require_group(
    plane: &Plane,
    game: Option<Uuid>,
    group_id: Uuid,
) -> Result<(), V2Error> {
    let Some(game) = game else {
        return Ok(());
    };
    let in_game = plane
        .q()
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

pub async fn require_hosted(plane: &Plane, claimed: Uuid) -> Result<(), V2Error> {
    let _ = crate::services::games::GamesService::new(Arc::new(plane.clone()))
        .find_game(claimed)
        .await?;
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
