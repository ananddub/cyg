use axum::Json;
use serde_json::json;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::db::DbError;
use crate::dto::Empty;
use crate::error::V2Error;
use crate::repositories::group::JoinGroupEdgeParams;
use crate::services::deps::q;
use crate::services::social;
use crate::utils::group::{ROLE_ADMIN, ROLE_MEMBER, group_id};
use crate::utils::guards::require_role;

use super::super::fetch_group;
use super::GroupMemberService;
use crate::dto::group::UsersQuery;

use super::users::resolve_users;

impl GroupMemberService {
    pub async fn add_group_users(
        &self,
        state: &ApiState,
        auth: &Authed,
        raw_id: String,
        req: UsersQuery,
    ) -> Result<Json<Empty>, V2Error> {
        let gid = group_id(&raw_id)?;
        let queries = q(state).await?;
        let group = fetch_group(&queries, gid).await?;
        require_role(&queries, auth.0.uuid()?, gid, ROLE_ADMIN).await?;
        if req.user_ids.is_none() {
            return Ok(Json(Empty {}));
        }
        let targets = resolve_users(&queries, &req).await?;

        for target in targets {
            let existing = match queries.get_group_edge_state(target, gid).await {
                Ok(role) => Some(role),
                Err(DbError::NotFound) => None,
                Err(e) => return Err(V2Error::from(e)),
            };
            if let Some(role) = existing {
                if role <= ROLE_MEMBER {
                    continue;
                }
            }
            if group.edge_count >= group.max_count {
                return Err(V2Error::invalid_argument("Group is full."));
            }
            queries
                .expand_group_edge_count(gid)
                .await
                .map_err(|e| match e {
                    DbError::NotFound => V2Error::invalid_argument("Group is full."),
                    other => V2Error::from(other),
                })?;
            queries
                .join_group_edge(&JoinGroupEdgeParams {
                    source_id: target,
                    destination_id: gid,
                    state: ROLE_MEMBER,
                    position: social::position_now(),
                })
                .await?;
            social::notify_with(
                state,
                target,
                auth.0.uuid()?,
                format!("You've been added to group {}", group.name),
                json!({ "group_id": social::uid(&gid), "name": group.name }),
                social::NOTIFY_GROUP_ADD,
            )
            .await;
        }
        Ok(Json(Empty {}))
    }
}
