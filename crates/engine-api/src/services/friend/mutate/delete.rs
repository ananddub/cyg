//! `apiFriendDelete` — `DELETE /v2/friend`.

use axum::Json;

use crate::ApiState;
use crate::dto::Empty;
use crate::services::auth::q;
use crate::error::V2Error;
use crate::app::middleware::Authed;
use crate::services::social;

use crate::dto::friend::TargetsQuery;
use crate::utils::friend::STATE_BLOCKED;

use super::resolve::{edge_state, resolve_targets};
use super::super::FriendService;

impl FriendService {
    /// `DELETE /v2/friend?ids=&usernames=` — the `DeleteFriends`: my edge always
    /// goes, theirs only when it is not a block of me; two rows deleted notifies them.
    pub async fn delete_friends(
        &self,
        state: &ApiState,
        auth: &Authed,
        req: TargetsQuery,
    ) -> Result<Json<Empty>, V2Error> {
        let queries = q(state)?;
        let me = auth.0.uuid()?;
        let targets =
            match resolve_targets(&queries, me, &auth.0.un, &req, "Cannot delete self.").await? {
                None => return Ok(Json(Empty {})),
                // BaaS logs this case and answers success: deleting nothing is not
                // an error, unlike adding or blocking an unresolvable target.
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
                        &queries,
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
