use axum::Json;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::group::{CreateGroupRequest, GroupEnvelope, GroupResponse};
use crate::error::V2Error;
use crate::repositories::group::{CreateGroupParams, JoinGroupEdgeParams};
use crate::services::deps::q;
use crate::services::social;
use crate::utils::group::{ROLE_SUPERADMIN, metadata_of};

use super::{GROUP_CLOSED, GROUP_OPEN, GroupService};

impl GroupService {
    pub async fn create_group(
        &self,
        state: &ApiState,
        auth: &Authed,
        body: CreateGroupRequest,
    ) -> Result<Json<GroupEnvelope>, V2Error> {
        if body.name.is_empty() {
            return Err(V2Error::invalid_argument("Group name must be set."));
        }
        let max_count = match body.max_count {
            None | Some(0) => 100,
            Some(n) if n < 1 => {
                return Err(V2Error::invalid_argument(
                    "Group max count must be >= 1 when set.",
                ));
            }
            Some(n) => n,
        };
        let queries = q(state).await?;
        let creator = match body.creator_id.as_deref() {
            None | Some("") => auth.0.uuid()?,
            Some(raw) => {
                let parsed = Uuid::parse_str(raw).unwrap_or(Uuid::nil());
                if parsed != auth.0.uuid()? {
                    return Err(V2Error::invalid_argument("Group creator invalid."));
                }
                parsed
            }
        };
        if let Ok(existing) = queries.get_group_by_name(&body.name).await {
            if existing.disable_time.timestamp() <= 0 {
                return Err(V2Error::already_exists("Group name is in use."));
            }
        }

        let open = body.open.unwrap_or(false);
        let group = queries
            .create_group(&CreateGroupParams {
                id: Uuid::new_v4(),
                creator_id: creator,
                name: body.name,
                description: body.description.unwrap_or_default(),
                avatar_url: body.avatar_url.unwrap_or_default(),
                lang_tag: body.lang_tag.unwrap_or_default(),
                metadata: metadata_of(body.metadata.as_ref())?,
                max_count,
                state: if open { GROUP_OPEN } else { GROUP_CLOSED },
            })
            .await?;
        queries
            .join_group_edge(&JoinGroupEdgeParams {
                source_id: creator,
                destination_id: group.id,
                state: ROLE_SUPERADMIN,
                position: social::position_now(),
            })
            .await?;
        Ok(Json(GroupEnvelope {
            group: GroupResponse::from_row(&group),
        }))
    }
}
