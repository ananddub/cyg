//! The `/v2/user/{user_id}/group` alias service.

use auto_di::singleton;
use axum::Json;
use serde_json::{Value, json};

use crate::ApiState;
use crate::dto::group::{UserGroupListResponse, UserGroupResponse};
use crate::repositories::group::{
    ListGroupsForUserByStatePagedParams, ListGroupsForUserPagedParams,
};
use crate::services::auth::q;
use crate::error::V2Error;
use crate::utils::group::{ROLE_BANNED, ROLE_MEMBER, ROLE_SUPERADMIN, as_groups, group_id};
use crate::app::middleware::Authed;
use crate::services::social::{self, limit};

use crate::dto::group::ListQuery;

pub struct UserGroupService;

#[singleton]
impl UserGroupService {
    fn new() -> Self {
        Self
    }
}

impl UserGroupService {
    /// `GET /v2/user/{user_id}/group?state=&limit=&cursor=` — `apiUserGroupList`.
    pub async fn list_user_groups(
        &self,
        state: &ApiState,
        _auth: &Authed,
        raw_user: String,
        req: ListQuery,
    ) -> Result<Json<UserGroupListResponse>, V2Error> {
        let uid = group_id(&raw_user)?;
        let queries = q(state)?;
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
        let params = ListGroupsForUserPagedParams {
            source_id: uid,
            state: state_filter,
            position,
            limit: page + 1,
        };

        let rows = if req.state.is_some() {
            queries
                .list_groups_for_user_by_state_paged(&ListGroupsForUserByStatePagedParams {
                    source_id: uid,
                    state: state_filter,
                    position,
                    limit: page + 1,
                })
                .await?
        } else {
            queries.list_groups_for_user_paged(&params).await?
        };
        let has_more = rows.len() as i64 > page;
        let mut rows = rows;
        if has_more {
            rows.truncate(page as usize);
        }
        let user_groups: Vec<UserGroupResponse> = rows
            .iter()
            .map(|row| {
                UserGroupResponse::new(&as_groups(row), row.member_state.unwrap_or(ROLE_MEMBER))
            })
            .collect();
        let next = rows
            .last()
            .map(|last| social::cursor_encode(&json!({ "state": i64::from(last.member_state.unwrap_or(0)), "position": last.member_position.unwrap_or(-1) })));
        Ok(Json(UserGroupListResponse {
            user_groups,
            cursor: if has_more { next.unwrap_or_default() } else { String::new() },
        }))
    }
}
