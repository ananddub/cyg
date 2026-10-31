use axum::Json;
use serde_json::{Value, json};

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::friend::{
    FoFQuery, FriendListResponse, FriendResponse, FriendsOfFriendsEntry, FriendsOfFriendsResponse,
    ListQuery,
};
use crate::error::V2Error;
use crate::repositories::friend::{
    ListFriendsOfFriendsParams, ListUserEdgesByStatePagedParams, ListUserEdgesPagedParams,
};
use crate::services::deps::q;
use crate::services::social::{self, limit};

use super::FriendService;
use crate::utils::friend::{STATE_BLOCKED, STATE_FRIENDS};

impl FriendService {
    pub async fn list_friends(
        &self,
        state: &ApiState,
        auth: &Authed,
        req: ListQuery,
    ) -> Result<Json<FriendListResponse>, V2Error> {
        let queries = q(state).await?;
        let me = auth.0.uuid()?;
        let page = limit(
            req.limit,
            1000,
            1000,
            "Invalid limit - limit must be between 1 and 1000.",
        )?;
        if let Some(state) = req.state {
            if !(STATE_FRIENDS..=STATE_BLOCKED).contains(&state) {
                return Err(V2Error::invalid_argument(
                    "Invalid state - state must be between 0 and 3.",
                ));
            }
        }

        const CURSOR_ERROR: &str = "Cursor is invalid.";
        let cursor = match req.cursor.as_deref() {
            None | Some("") => Value::Null,
            Some(raw) => social::cursor_decode(raw, CURSOR_ERROR)?,
        };
        if req.state.is_some() && !cursor.is_null() {
            let cursor_state = social::cursor_i64(&cursor, "state", -1);
            if cursor_state != i64::from(req.state.unwrap_or(-1)) {
                return Err(V2Error::invalid_argument(CURSOR_ERROR));
            }
        }

        let state_filter = req.state.unwrap_or(-1);
        let position = social::cursor_i64(&cursor, "position", -1);
        let mut rows = if req.state.is_some() {
            queries
                .list_user_edges_by_state_paged(&ListUserEdgesByStatePagedParams {
                    source_id: me,
                    state: state_filter,
                    position,
                    limit: page + 1,
                })
                .await?
        } else {
            queries
                .list_user_edges_paged(&ListUserEdgesPagedParams {
                    source_id: me,
                    state: state_filter,
                    position,
                    limit: page + 1,
                })
                .await?
        };

        let has_more = rows.len() as i64 > page;
        if has_more {
            rows.truncate(page as usize);
        }
        let friends: Vec<FriendResponse> = rows.iter().map(FriendResponse::from_row).collect();
        let next = rows.last().map(|last| {
            social::cursor_encode(&json!({
                "state": i64::from(last.state.unwrap_or(STATE_FRIENDS)),
                "position": last.edge_position.unwrap_or(-1),
            }))
        });
        Ok(Json(FriendListResponse {
            friends,
            cursor: if has_more {
                next.unwrap_or_default()
            } else {
                String::new()
            },
        }))
    }

    pub async fn list_friends_of_friends(
        &self,
        state: &ApiState,
        auth: &Authed,
        req: FoFQuery,
    ) -> Result<Json<FriendsOfFriendsResponse>, V2Error> {
        let queries = q(state).await?;
        let page = limit(
            req.limit,
            10,
            100,
            "Invalid limit - limit must be between 1 and 100.",
        )?;
        let cursor =
            social::cursor_decode(req.cursor.as_deref().unwrap_or(""), "Cursor is invalid.")?;
        let mut rows = queries
            .list_friends_of_friends(&ListFriendsOfFriendsParams {
                source_id: auth.0.uuid()?,
                source_id2: social::cursor_uuid(&cursor, "referrer"),
                destination_id_start: social::cursor_uuid(&cursor, "id"),
                limit: page + 1,
            })
            .await?;
        let has_more = rows.len() as i64 > page;
        if has_more {
            rows.truncate(page as usize);
        }
        let friends_of_friends: Vec<FriendsOfFriendsEntry> = rows
            .iter()
            .filter_map(|row| {
                let id = row.id?;
                Some(FriendsOfFriendsEntry::from_row(row, id))
            })
            .collect();
        let next = if has_more {
            rows.last().and_then(|last| {
                let referrer = last.referrer?;
                let id = last.id?;
                Some(social::cursor_encode(
                    &json!({ "referrer": social::uid(&referrer), "id": social::uid(&id) }),
                ))
            })
        } else {
            None
        };
        Ok(Json(FriendsOfFriendsResponse {
            friends_of_friends,
            cursor: next.unwrap_or_default(),
        }))
    }
}
