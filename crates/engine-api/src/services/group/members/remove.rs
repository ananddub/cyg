use axum::Json;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::db::DbError;
use crate::dto::Empty;
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::repositories::group::BanGroupEdgeParams;
use crate::services::deps::q;
use crate::services::social;
use crate::utils::group::{ROLE_ADMIN, ROLE_MEMBER, group_id};
use crate::utils::guards::require_role;

use super::super::fetch_group;
use super::GroupMemberService;
use crate::dto::group::UsersQuery;

use super::users::resolve_users;

impl GroupMemberService {
    pub async fn kick_group_users(
        &self,
        state: &ApiState,
        auth: &Authed,
        raw_id: String,
        req: UsersQuery,
    ) -> Result<Json<Empty>, V2Error> {
        let queries = q(state).await?;
        let gid = group_id(&raw_id)?;
        fetch_group(&queries, gid).await?;
        remove_group_users(&queries, auth.0.uuid()?, gid, &req, false).await
    }

    pub async fn ban_group_users(
        &self,
        state: &ApiState,
        auth: &Authed,
        raw_id: String,
        req: UsersQuery,
    ) -> Result<Json<Empty>, V2Error> {
        let queries = q(state).await?;
        let gid = group_id(&raw_id)?;
        fetch_group(&queries, gid).await?;
        remove_group_users(&queries, auth.0.uuid()?, gid, &req, true).await
    }
}

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
            Err(DbError::NotFound) => continue,
            Err(e) => return Err(V2Error::from(e)),
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
