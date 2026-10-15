//! `apiGroupUserList` — `GET /v2/group/{group_id}/user`.

use axum::Json;
use serde_json::{Value, json};

use crate::ApiState;
use crate::dto::group::{GroupUserListResponse, GroupUserResponse};
use crate::error::V2Error;
use crate::repositories::group::{ListGroupUsersByStatePagedParams, ListGroupUsersPagedParams};
use crate::services::auth::q;
use crate::app::middleware::Authed;
use crate::services::social::{self, limit};
use crate::utils::group::{ROLE_BANNED, ROLE_SUPERADMIN, group_id};

use crate::dto::group::ListQuery;

use super::super::fetch_group;
use super::GroupMemberService;

impl GroupMemberService {
    /// `GET /v2/group/{group_id}/user?state=&limit=&cursor=` — `apiGroupUserList`.
    pub async fn list_group_users(
        &self,
        state: &ApiState,
        _auth: &Authed,
        raw_id: String,
        req: ListQuery,
    ) -> Result<Json<GroupUserListResponse>, V2Error> {
        let gid = group_id(&raw_id)?;
        let queries = q(state)?;
        fetch_group(&queries, gid).await?;
        let page = limit(
            req.limit,
            100,
            100,
            "Invalid limit - limit must be between 1 and 100.",
        )?;
        if let Some(role) = req.state {
            if !(ROLE_SUPERADMIN..=ROLE_BANNED).contains(&role) {
                return Err(V2Error::invalid_argument(
                    "Invalid state - state must be between 0 and 4.",
                ));
            }
        }

        const CURSOR_ERROR: &str = "Cursor is invalid.";
        let cursor = match req.cursor.as_deref() {
            None | Some("") => Value::Null,
            Some(raw) => social::cursor_decode(raw, CURSOR_ERROR)?,
        };
        if req.state.is_some()
            && !cursor.is_null()
            && social::cursor_i64(&cursor, "state", -1) != i64::from(req.state.unwrap_or(-1))
        {
            return Err(V2Error::invalid_argument(CURSOR_ERROR));
        }
        let state_filter = req.state.unwrap_or(-1);
        let position = social::cursor_i64(&cursor, "position", -1);

        let rows = if req.state.is_some() {
            queries
                .list_group_users_by_state_paged(&ListGroupUsersByStatePagedParams {
                    destination_id: gid,
                    state: state_filter,
                    position,
                    limit: page + 1,
                })
                .await?
        } else {
            queries
                .list_group_users_paged(&ListGroupUsersPagedParams {
                    destination_id: gid,
                    state: state_filter,
                    position,
                    limit: page + 1,
                })
                .await?
        };
        let has_more = rows.len() as i64 > page;
        let mut rows = rows;
        if has_more {
            rows.truncate(page as usize);
        }
        let group_users: Vec<GroupUserResponse> =
            rows.iter().map(GroupUserResponse::from_row).collect();
        let next = rows
            .last()
            .map(|last| social::cursor_encode(&json!({ "state": i64::from(last.state.unwrap_or(0)), "position": last.edge_position.unwrap_or(-1) })));
        Ok(Json(GroupUserListResponse {
            group_users,
            cursor: if has_more { next.unwrap_or_default() } else { String::new() },
        }))
    }
}
