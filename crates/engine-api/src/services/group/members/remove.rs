//! Kick and ban — `POST /v2/group/{group_id}/kick` and `/ban`.

use axum::Json;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::Empty;
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::repositories::group::BanGroupEdgeParams;
use crate::services::auth::q;
use crate::services::social;
use crate::utils::group::{ROLE_ADMIN, ROLE_MEMBER, group_id};

use super::super::{fetch_group, require_role};
use super::GroupMemberService;
use crate::dto::group::UsersQuery;

use super::users::resolve_users;

impl GroupMemberService {
    /// `POST /v2/group/{group_id}/kick?user_ids=`.
    pub async fn kick_group_users(
        &self,
        state: &ApiState,
        auth: &Authed,
        raw_id: String,
        req: UsersQuery,
    ) -> Result<Json<Empty>, V2Error> {
        let queries = q(state)?;
        let gid = group_id(&raw_id)?;
        fetch_group(&queries, gid).await?;
        remove_group_users(&queries, auth.0.uuid()?, gid, &req, false).await
    }

    /// `POST /v2/group/{group_id}/ban?user_ids=`.
    pub async fn ban_group_users(
        &self,
        state: &ApiState,
        auth: &Authed,
        raw_id: String,
        req: UsersQuery,
    ) -> Result<Json<Empty>, V2Error> {
        let queries = q(state)?;
        let gid = group_id(&raw_id)?;
        fetch_group(&queries, gid).await?;
        remove_group_users(&queries, auth.0.uuid()?, gid, &req, true).await
    }
}

/// Kick removes the edge, ban replaces it with state 4; neither may touch a role at or
/// above the caller's, and the count only falls for a row that held a slot.
async fn remove_group_users(
    queries: &Queries,
    caller: Uuid,
    gid: Uuid,
    req: &UsersQuery,
    ban: bool,
) -> Result<Json<Empty>, V2Error> {
    let max_role = require_role(queries, caller, gid, ROLE_ADMIN).await?;
    if req.user_ids.is_none() {
        return Ok(Json(Empty {}));
    }
    let targets = resolve_users(queries, req).await?;
    for target in targets {
        let role = match queries.get_group_edge_state(target, gid).await {
            Ok(role) => role,
            Err(sqlx::Error::RowNotFound) => continue,
            Err(e) => return Err(V2Error::from_sqlx(e)),
        };
        if role <= max_role {
            return Err(V2Error::invalid_argument(
                "Cannot remove a user of equal or higher role.",
            ));
        }
        if ban {
            queries
                .ban_group_edge(&BanGroupEdgeParams {
                    source_id: target,
                    destination_id: gid,
                    position: social::position_now(),
                })
                .await?;
        } else {
            queries.delete_group_edge(target, gid).await?;
        }
        if role <= ROLE_MEMBER {
            queries.shrink_group_edge_count(gid).await?;
        }
    }
    Ok(Json(Empty {}))
}
