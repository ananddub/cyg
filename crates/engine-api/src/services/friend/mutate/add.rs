//! `apiFriendAdd` — `POST /v2/friend`.

use axum::Json;
use serde_json::{Value, json};

use crate::ApiState;
use crate::dto::Empty;
use crate::repositories::friend::{
    IncrementPairEdgeCountParams, InsertFriendEdgeParams, UpdateFriendEdgeMetadataParams,
};
use crate::services::auth::q;
use crate::error::V2Error;
use crate::app::middleware::Authed;
use crate::services::social;

use crate::dto::friend::TargetsQuery;
use crate::utils::friend::{STATE_BLOCKED, STATE_INVITE_RECEIVED, STATE_INVITE_SENT};

use super::resolve::{edge_state, resolve_targets};
use super::super::FriendService;

impl FriendService {
    /// `POST /v2/friend?ids=&usernames=&metadata=` — the `AddFriends`.
    pub async fn add_friends(
        &self,
        state: &ApiState,
        auth: &Authed,
        req: TargetsQuery,
    ) -> Result<Json<Empty>, V2Error> {
        let queries = q(state)?;
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
            // The core guards its insert with `EXISTS (SELECT id FROM users …)`.
            match queries.get_user_by_id(target).await {
                Ok(_) => {}
                Err(sqlx::Error::RowNotFound) => continue,
                Err(e) => return Err(V2Error::from_sqlx(e)),
            }

            let mine = edge_state(&queries, me, target).await?;
            let theirs = edge_state(&queries, target, me).await?;
            // Blocked in either direction, the request is ignored rather than
            // refused — the only signal a blocked player gets is that nothing
            // happens (core_friend.go `continue`s on a block it finds).
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
                    &queries,
                    target,
                    me,
                    &auth.0.un,
                    format!("{} accepted your friend request", auth.0.un),
                    social::NOTIFY_FRIEND_ACCEPT,
                )
                .await;
                continue;
            }

            // A pair already in place re-adds nothing, and only a brand-new pair
            // moves the edge counts — which `IncrementPairEdgeCount` enforces by
            // matching the position these two rows were just written with.
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
                    &queries,
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
