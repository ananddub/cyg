//! `apiFriendBlock` — `POST /v2/friend/block`.

use axum::Json;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::Empty;
use crate::error::V2Error;
use crate::repositories::friend::BlockUserEdgesParams;
use crate::services::auth::q;
use crate::services::social;

use crate::dto::friend::TargetsQuery;
use crate::utils::friend::STATE_BLOCKED;

use super::super::FriendService;
use super::resolve::{edge_state, resolve_targets};

impl FriendService {
    /// `POST /v2/friend/block?ids=&usernames=` — the `BlockFriends`: a block is
    /// one-directional, and it destroys their edge to me unless they blocked me first.
    pub async fn block_friends(
        &self,
        state: &ApiState,
        auth: &Authed,
        req: TargetsQuery,
    ) -> Result<Json<Empty>, V2Error> {
        let queries = q(state)?;
        let me = auth.0.uuid()?;
        let targets =
            match resolve_targets(&queries, me, &auth.0.un, &req, "Cannot block self.").await? {
                None => return Ok(Json(Empty {})),
                Some(targets) if targets.is_empty() => {
                    return Err(V2Error::invalid_argument(
                        "No valid ID or username was provided.",
                    ));
                }
                Some(targets) => targets,
            };

        for target in targets {
            let mine = edge_state(&queries, me, target).await?;
            if mine.is_none() {
                // BaaS inserts the block only when the target account exists.
                match queries.get_user_by_id(target).await {
                    Ok(_) => {}
                    Err(sqlx::Error::RowNotFound) => continue,
                    Err(e) => return Err(V2Error::from_sqlx(e)),
                }
            }
            queries
                .block_user_edges(&BlockUserEdgesParams {
                    source_id: me,
                    destination_id: target,
                    position: social::position_now(),
                })
                .await?;
            if mine.is_none() {
                queries.increment_user_edge_count(1, me).await?;
            }

            if let Some(state) = edge_state(&queries, target, me).await? {
                if state != STATE_BLOCKED {
                    queries.delete_user_edge(target, me).await?;
                    queries.increment_user_edge_count(-1, target).await?;
                }
            }
        }

        Ok(Json(Empty {}))
    }
}
