use axum::Json;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::Empty;
use crate::error::V2Error;
use crate::services::deps::q;
use crate::services::social;

use crate::dto::friend::TargetsQuery;
use crate::utils::friend::STATE_BLOCKED;

use super::super::FriendService;
use super::resolve::{edge_state, resolve_targets};

impl FriendService {
    pub async fn delete_friends(
        &self,
        state: &ApiState,
        auth: &Authed,
        req: TargetsQuery,
    ) -> Result<Json<Empty>, V2Error> {
        let queries = q(state).await?;
        let me = auth.0.uuid()?;
        let targets =
            match resolve_targets(&queries, me, &auth.0.un, &req, "Cannot delete self.").await? {
                None => return Ok(Json(Empty {})),
                Some(targets) => targets,
            };

        for target in targets {
            let mut deleted = 0;
            if edge_state(&queries, me, target).await?.is_some() {
                queries.delete_user_edge(me, target).await?;
                deleted += 1;
            }
            match edge_state(&queries, target, me).await? {
                Some(STATE_BLOCKED) | None => {}
                Some(_) => {
                    queries.delete_user_edge(target, me).await?;
                    deleted += 1;
                }
            }
            match deleted {
                1 => queries.increment_user_edge_count(-1, me).await?,
                2 => {
                    queries.increment_user_edge_count(-1, me).await?;
                    queries.increment_user_edge_count(-1, target).await?;
                    social::notify(
                        state,
                        target,
                        me,
                        &auth.0.un,
                        format!("{} removed you as a friend", auth.0.un),
                        social::NOTIFY_FRIEND_REMOVE,
                    )
                    .await;
                }
                _ => {}
            }
        }

        Ok(Json(Empty {}))
    }
}
