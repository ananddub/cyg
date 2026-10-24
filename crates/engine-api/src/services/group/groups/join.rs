//! `JoinGroup` and `LeaveGroup`.

use axum::Json;
use serde_json::json;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::Empty;
use crate::error::V2Error;
use crate::repositories::group::JoinGroupEdgeParams;
use crate::services::auth::q;
use crate::services::social;
use crate::utils::group::{ROLE_ADMIN, ROLE_JOIN_REQUEST, ROLE_MEMBER, ROLE_SUPERADMIN, group_id};

use super::super::fetch_group;
use super::{GROUP_OPEN, GroupService};

impl GroupService {
    /// `POST /v2/group/{group_id}/join`. An open group admits the caller as a member; a
    /// closed one records a join request (a no-op for an existing member, as in BaaS).
    pub async fn join_group(
        &self,
        state: &ApiState,
        auth: &Authed,
        raw_id: String,
    ) -> Result<Json<Empty>, V2Error> {
        let gid = group_id(&raw_id)?;
        let queries = q(state)?;
        let group = fetch_group(&queries, gid).await?;
        let uid = auth.0.uuid()?;

        if let Ok(existing) = queries.get_group_edge_state(uid, gid).await {
            if existing <= ROLE_MEMBER {
                return Ok(Json(Empty {}));
            }
            if existing == ROLE_JOIN_REQUEST {
                return Ok(Json(Empty {}));
            }
            return Err(V2Error::permission_denied("Banned from the group."));
        }

        let open = group.state == GROUP_OPEN;
        if open && group.edge_count >= group.max_count {
            return Err(V2Error::invalid_argument("Group is full."));
        }
        if open {
            // Capacity is claimed by the counter itself: it refuses past `max_count`.
            queries
                .expand_group_edge_count(gid)
                .await
                .map_err(|e| match e {
                    sqlx::Error::RowNotFound => V2Error::invalid_argument("Group is full."),
                    other => V2Error::from_sqlx(other),
                })?;
        }
        queries
            .join_group_edge(&JoinGroupEdgeParams {
                source_id: uid,
                destination_id: gid,
                state: if open { ROLE_MEMBER } else { ROLE_JOIN_REQUEST },
                position: social::position_now(),
            })
            .await?;

        if !open {
            let content = json!({ "group_id": social::uid(&gid), "username": auth.0.un });
            for admin in queries
                .list_group_admins(gid, ROLE_ADMIN)
                .await
                .unwrap_or_default()
            {
                social::notify_with(
                    &queries,
                    admin,
                    uid,
                    format!("User {} wants to join your group", auth.0.un),
                    content.clone(),
                    social::NOTIFY_GROUP_JOIN_REQUEST,
                )
                .await;
            }
        }
        Ok(Json(Empty {}))
    }

    /// `POST /v2/group/{group_id}/leave`. The last superadmin cannot walk out of a
    /// group they still own.
    pub async fn leave_group(
        &self,
        state: &ApiState,
        auth: &Authed,
        raw_id: String,
    ) -> Result<Json<Empty>, V2Error> {
        let gid = group_id(&raw_id)?;
        let queries = q(state)?;
        fetch_group(&queries, gid).await?;
        let uid = auth.0.uuid()?;
        let role = queries
            .get_group_edge_state(uid, gid)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => V2Error::not_found("Group not found."),
                other => V2Error::from_sqlx(other),
            })?;
        if role == ROLE_SUPERADMIN {
            let holders = queries
                .count_group_role_holders(gid, ROLE_SUPERADMIN)
                .await?;
            if holders <= 1 {
                return Err(V2Error::invalid_argument(
                    "Cannot leave group when you are the last superadmin.",
                ));
            }
        }
        queries.leave_group_edge(uid, gid).await?;
        if role <= ROLE_MEMBER {
            queries.shrink_group_edge_count(gid).await?;
        }
        Ok(Json(Empty {}))
    }
}
