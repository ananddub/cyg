//! Promote and demote — `POST /v2/group/{group_id}/promote` and `/demote`.

use axum::Json;
use uuid::Uuid;

use crate::ApiState;
use crate::dto::Empty;
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::repositories::group::{DemoteGroupEdgeParams, PromoteGroupEdgeParams};
use crate::services::auth::q;
use crate::app::middleware::Authed;
use crate::utils::group::{ROLE_ADMIN, group_id};

use super::super::{fetch_group, require_role};
use super::GroupMemberService;
use crate::dto::group::UsersQuery;

use super::users::resolve_users;

impl GroupMemberService {
    /// `POST /v2/group/{group_id}/promote?user_ids=` — one step up the ladder, and
    /// never past the caller's own rank.
    pub async fn promote_group_users(
        &self,
        state: &ApiState,
        auth: &Authed,
        raw_id: String,
        req: UsersQuery,
    ) -> Result<Json<Empty>, V2Error> {
        let queries = q(state)?;
        let gid = group_id(&raw_id)?;
        fetch_group(&queries, gid).await?;
        shift_group_roles(&queries, auth.0.uuid()?, gid, &req, true).await
    }

    /// `POST /v2/group/{group_id}/demote?user_ids=`.
    pub async fn demote_group_users(
        &self,
        state: &ApiState,
        auth: &Authed,
        raw_id: String,
        req: UsersQuery,
    ) -> Result<Json<Empty>, V2Error> {
        let queries = q(state)?;
        let gid = group_id(&raw_id)?;
        fetch_group(&queries, gid).await?;
        shift_group_roles(&queries, auth.0.uuid()?, gid, &req, false).await
    }
}

async fn shift_group_roles(
    queries: &Queries,
    caller: Uuid,
    gid: Uuid,
    req: &UsersQuery,
    promote: bool,
) -> Result<Json<Empty>, V2Error> {
    let max_role = require_role(queries, caller, gid, ROLE_ADMIN).await?;
    if req.user_ids.is_none() {
        return Ok(Json(Empty {}));
    }
    let targets = resolve_users(queries, req).await?;
    for target in targets {
        if promote {
            queries
                .promote_group_edge(&PromoteGroupEdgeParams {
                    destination_id: gid,
                    source_id: target,
                    state: max_role,
                })
                .await?;
        } else {
            queries
                .demote_group_edge(&DemoteGroupEdgeParams {
                    destination_id: gid,
                    source_id: target,
                    state: max_role,
                })
                .await?;
        }
    }
    Ok(Json(Empty {}))
}
