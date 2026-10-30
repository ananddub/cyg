use axum::Json;
use serde_json::{Value, json};

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::db::DbError;
use crate::dto::Empty;
use crate::error::V2Error;
use crate::repositories::friend::{
    IncrementPairEdgeCountParams, InsertFriendEdgeParams, UpdateFriendEdgeMetadataParams,
};
use crate::services::deps::q;
use crate::services::social;

use crate::dto::friend::TargetsQuery;
use crate::utils::friend::{STATE_BLOCKED, STATE_INVITE_RECEIVED, STATE_INVITE_SENT};

use super::super::FriendService;
use super::resolve::{edge_state, resolve_targets};

impl FriendService {
    pub async fn add_friends(
        &self,
        state: &ApiState,
        auth: &Authed,
        req: TargetsQuery,
    ) -> Result<Json<Empty>, V2Error> {
        let queries = q(state).await?;
        let me = auth.0.uuid()?;
        let targets =
            match resolve_targets(&queries, me, &auth.0.un, &req, "Cannot add self as friend.")
                .await?
            {
                None => return Ok(Json(Empty {})),
                Some(targets) if targets.is_empty() => {
                    return Err(V2Error::invalid_argument(
                        "No valid ID or username was provided.",
                    ));
                }
                Some(targets) => targets,
            };
        let metadata = req
            .metadata
            .as_deref()
            .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
            .unwrap_or_else(|| json!({}));

        for target in targets {
            match queries.get_user_by_id(target).await {
                Ok(_) => {}
                Err(DbError::NotFound) => continue,
                Err(e) => return Err(V2Error::from(e)),
            }

            let mine = edge_state(&queries, me, target).await?;
            let theirs = edge_state(&queries, target, me).await?;
            if mine == Some(STATE_BLOCKED) || theirs == Some(STATE_BLOCKED) {
                continue;
            }

            if mine == Some(STATE_INVITE_RECEIVED) || theirs == Some(STATE_INVITE_SENT) {
                queries.accept_friend_invite(me, target).await?;
                if metadata.as_object().map(|m| !m.is_empty()).unwrap_or(false) {
                    queries
                        .update_friend_edge_metadata(&UpdateFriendEdgeMetadataParams {
                            metadata: metadata.clone(),
                            source_id: me,
                            destination_id: target,
                        })
                        .await?;
                }
                social::notify(
                    state,
                    target,
                    me,
                    &auth.0.un,
                    format!("{} accepted your friend request", auth.0.un),
                    social::NOTIFY_FRIEND_ACCEPT,
                )
                .await;
                continue;
            }

            if mine.is_none() && theirs.is_none() {
                let position = social::position_now();
                queries
                    .insert_friend_edge(&InsertFriendEdgeParams {
                        source_id: me,
                        destination_id: target,
                        state: STATE_INVITE_SENT,
                        position,
                        metadata: metadata.clone(),
                    })
                    .await?;
                queries
                    .insert_friend_edge(&InsertFriendEdgeParams {
                        source_id: target,
                        destination_id: me,
                        state: STATE_INVITE_RECEIVED,
                        position,
                        metadata: json!({}),
                    })
                    .await?;
                queries
                    .increment_pair_edge_count(&IncrementPairEdgeCountParams {
                        id: me,
                        id2: target,
                        position,
                    })
                    .await?;
                social::notify(
                    state,
                    target,
                    me,
                    &auth.0.un,
                    format!("{} wants to add you as a friend", auth.0.un),
                    social::NOTIFY_FRIEND_REQUEST,
                )
                .await;
            }
        }

        Ok(Json(Empty {}))
    }
}
