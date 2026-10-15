//! `UpdateGroup` and `DeleteGroup`.

use axum::Json;

use crate::ApiState;
use crate::dto::Empty;
use crate::dto::group::UpdateGroupRequest;
use crate::repositories::group::UpdateGroupParams;
use crate::services::auth::q;
use crate::error::V2Error;
use crate::utils::group::{ROLE_SUPERADMIN, group_id, metadata_of};
use crate::app::middleware::Authed;

use super::super::{fetch_group, require_role};
use super::{GROUP_CLOSED, GROUP_OPEN, GroupService};

impl GroupService {
    /// `PUT /v2/group/{group_id}` — `apiUpdateGroup`.
    pub async fn update_group(
        &self,
        state: &ApiState,
        auth: &Authed,
        raw_id: String,
        body: UpdateGroupRequest,
    ) -> Result<Json<Empty>, V2Error> {
        let gid = group_id(&raw_id)?;
        let queries = q(state)?;
        let group = fetch_group(&queries, gid).await?;
        require_role(&queries, auth.0.uuid()?, gid, ROLE_SUPERADMIN).await?;
        if body.name.is_none()
            && body.description.is_none()
            && body.avatar_url.is_none()
            && body.lang_tag.is_none()
            && body.metadata.is_none()
            && body.max_count.is_none()
            && body.open.is_none()
        {
            return Err(V2Error::invalid_argument(
                "Specify at least one field to update.",
            ));
        }
        if let Some(name) = body.name.as_deref() {
            if name != group.name {
                if let Ok(taken) = queries.get_group_by_name(name).await {
                    if taken.disable_time.timestamp() <= 0 {
                        return Err(V2Error::invalid_argument("Group name is in use."));
                    }
                }
            }
        }

        let metadata = match body.metadata.as_ref() {
            None => group.metadata.clone(),
            Some(_) => metadata_of(body.metadata.as_ref())?,
        };
        queries
            .update_group(&UpdateGroupParams {
                name: body.name.clone().unwrap_or(group.name.clone()),
                description: body
                    .description
                    .clone()
                    .unwrap_or(group.description.clone().unwrap_or_default()),
                avatar_url: body
                    .avatar_url
                    .clone()
                    .unwrap_or(group.avatar_url.clone().unwrap_or_default()),
                lang_tag: body.lang_tag.clone().unwrap_or(group.lang_tag.clone()),
                metadata,
                max_count: match body.max_count {
                    None => group.max_count,
                    Some(n) if n < 1 => {
                        return Err(V2Error::invalid_argument(
                            "Group max count must be >= 1 when set.",
                        ));
                    }
                    Some(n) => n,
                },
                id: gid,
            })
            .await?;

        // `open` is the one group setting that is not part of the generated update.
        if let Some(open) = body.open {
            let db = state.db.as_ref().ok_or_else(V2Error::no_db)?;
            sqlx::query("UPDATE groups SET state = $1, update_time = now() WHERE id = $2")
                .bind(if open { GROUP_OPEN } else { GROUP_CLOSED })
                .bind(gid)
                .execute(db)
                .await?;
        }
        Ok(Json(Empty {}))
    }

    /// `DELETE /v2/group/{group_id}` — `apiDeleteGroup`.
    pub async fn delete_group(
        &self,
        state: &ApiState,
        auth: &Authed,
        raw_id: String,
    ) -> Result<Json<Empty>, V2Error> {
        let gid = group_id(&raw_id)?;
        let queries = q(state)?;
        fetch_group(&queries, gid).await?;
        require_role(&queries, auth.0.uuid()?, gid, ROLE_SUPERADMIN).await?;
        queries.delete_group(gid).await?;
        Ok(Json(Empty {}))
    }
}
