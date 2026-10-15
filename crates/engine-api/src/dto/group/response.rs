//! The group response shapes.

use std::borrow::Cow;

use serde::Serialize;

use crate::dto::user::{UserCols, UserResponse};
use crate::repositories::group::ListGroupUsersPagedRow;
use crate::repositories::models::Groups;
use crate::services::social::{meta_str, ts, uid};

/// the `apiGroup`. `groups.state` is `0=open, 1=closed`, and the wire name for it
/// is the boolean `open`.
#[derive(Debug, Serialize)]
pub struct GroupResponse {
    pub id: String,
    pub creator_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang_tag: Option<String>,
    pub metadata: String,
    pub open: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_count: Option<i32>,
    pub create_time: String,
    pub update_time: String,
}

impl GroupResponse {
    pub fn from_row(group: &Groups) -> Self {
        GroupResponse {
            id: uid(&group.id),
            creator_id: uid(&group.creator_id),
            name: group.name.clone(),
            description: non_empty(group.description.as_deref()),
            avatar_url: non_empty(group.avatar_url.as_deref()),
            lang_tag: non_empty(Some(&group.lang_tag)),
            metadata: meta_str(&group.metadata),
            open: group.state == 0,
            edge_count: (group.edge_count != 0).then_some(group.edge_count),
            max_count: (group.max_count != 0).then_some(group.max_count),
            create_time: ts(group.create_time),
            update_time: ts(group.update_time),
        }
    }
}

/// `apiGetGroup` and `CreateGroup`: the one group they answer with.
#[derive(Debug, Serialize)]
pub struct GroupEnvelope {
    pub group: GroupResponse,
}

/// `ListGroups`: the page plus the cursor to resume at. An empty cursor is still a
/// member, as the gateway marshals it.
#[derive(Debug, Serialize)]
pub struct GroupListResponse {
    pub groups: Vec<GroupResponse>,
    pub cursor: String,
}

/// the `apiUserGroup`: the group and the caller's edge state in it.
#[derive(Debug, Serialize)]
pub struct UserGroupResponse {
    pub group: GroupResponse,
    pub state: i16,
}

impl UserGroupResponse {
    pub fn new(group: &Groups, member_state: i16) -> Self {
        UserGroupResponse {
            group: GroupResponse::from_row(group),
            state: member_state,
        }
    }
}

/// `ListGroupsOfUser`: the page plus the cursor to resume at.
#[derive(Debug, Serialize)]
pub struct UserGroupListResponse {
    pub user_groups: Vec<UserGroupResponse>,
    pub cursor: String,
}

/// the `apiGroupUser`: a member's user object plus the edge `state`.
#[derive(Debug, Serialize)]
pub struct GroupUserResponse {
    pub user: UserResponse,
    pub state: i16,
}

impl GroupUserResponse {
    pub fn from_row(row: &ListGroupUsersPagedRow) -> Self {
        let cols = UserCols {
            id: Cow::Owned(uid(&row.id)),
            username: &row.username,
            display_name: row.display_name.as_deref(),
            avatar_url: row.avatar_url.as_deref(),
            lang_tag: "",
            location: None,
            timezone: None,
            metadata: &row.metadata,
            edge_count: None,
            create_time: row.create_time,
            update_time: row.update_time,
        };
        GroupUserResponse {
            user: UserResponse::from_cols(&cols),
            state: row.state.unwrap_or(crate::utils::group::ROLE_MEMBER),
        }
    }
}

/// `ListGroupUsers`: the page plus the cursor to resume at.
#[derive(Debug, Serialize)]
pub struct GroupUserListResponse {
    pub group_users: Vec<GroupUserResponse>,
    pub cursor: String,
}

/// `""` is dropped from the wire, so it folds into "absent".
fn non_empty(value: Option<&str>) -> Option<String> {
    value.filter(|s| !s.is_empty()).map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uuid::Uuid;

    fn group() -> Groups {
        let t = chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        Groups {
            id: Uuid::nil(),
            creator_id: Uuid::nil(),
            name: "the-crew".to_owned(),
            description: None,
            avatar_url: Some(String::new()),
            lang_tag: String::new(),
            metadata: json!({}),
            state: 0,
            edge_count: 3,
            max_count: 100,
            create_time: t,
            update_time: t,
            disable_time: t,
        }
    }

    #[test]
    fn open_is_a_boolean_and_empty_members_are_absent() {
        assert_eq!(
            serde_json::to_string(&GroupResponse::from_row(&group())).unwrap(),
            concat!(
                r#"{"id":"00000000-0000-0000-0000-000000000000","creator_id":"00000000-0000-0000-0000-000000000000","#,
                r#""name":"the-crew","metadata":"{}","open":true,"edge_count":3,"max_count":100,"#,
                r#""create_time":"2023-11-14T22:13:20Z","update_time":"2023-11-14T22:13:20Z"}"#,
            )
        );
    }

    #[test]
    fn a_user_group_nests_the_group_under_a_group_key() {
        let response = UserGroupResponse::new(&group(), 0);
        assert_eq!(
            serde_json::to_string(&response).unwrap(),
            format!(r#"{{"group":{},"state":0}}"#, serde_json::to_string(&GroupResponse::from_row(&group())).unwrap())
        );
    }
}
