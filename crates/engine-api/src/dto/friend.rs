use std::borrow::Cow;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::repositories::friend::{ListFriendsOfFriendsRow, ListUserEdgesPagedRow};
use crate::services::social::{meta_str, ts, uid};
use crate::utils::friend::STATE_FRIENDS;

use super::user::{UserCols, UserResponse};

#[derive(Debug, Serialize)]
pub struct FriendResponse {
    pub user: UserResponse,
    pub state: i16,
    pub update_time: String,
    pub metadata: String,
}

impl FriendResponse {
    pub fn new(
        user: UserResponse,
        state: i16,
        edge_update_time: DateTime<Utc>,
        metadata: &Value,
    ) -> Self {
        FriendResponse {
            user,
            state,
            update_time: ts(edge_update_time),
            metadata: meta_str(metadata),
        }
    }

    pub fn from_row(row: &ListUserEdgesPagedRow) -> Self {
        let cols = UserCols {
            id: Cow::Owned(uid(&row.id)),
            username: &row.username,
            display_name: row.display_name.as_deref(),
            avatar_url: row.avatar_url.as_deref(),
            lang_tag: &row.lang_tag,
            location: row.location.as_deref(),
            timezone: row.timezone.as_deref(),
            metadata: &row.metadata,
            edge_count: None,
            create_time: row.create_time,
            update_time: row.update_time,
        };
        FriendResponse::new(
            UserResponse::from_cols(&cols),
            row.state.unwrap_or(STATE_FRIENDS),
            row.edge_update_time.unwrap_or(row.create_time),
            row.edge_metadata
                .as_ref()
                .unwrap_or(&Value::Object(Default::default())),
        )
    }
}

#[derive(Debug, Serialize)]
pub struct FriendListResponse {
    pub friends: Vec<FriendResponse>,
    pub cursor: String,
}

#[derive(Debug, Serialize)]
pub struct FriendsOfFriendsEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub referrer: Option<String>,
    pub user: UserResponse,
}

impl FriendsOfFriendsEntry {
    pub fn from_row(row: &ListFriendsOfFriendsRow, id: uuid::Uuid) -> Self {
        let cols = UserCols {
            id: Cow::Owned(uid(&id)),
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
        FriendsOfFriendsEntry {
            referrer: row.referrer.map(|referrer| uid(&referrer)),
            user: UserResponse::from_cols(&cols),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct FriendsOfFriendsResponse {
    pub friends_of_friends: Vec<FriendsOfFriendsEntry>,
    pub cursor: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub state: Option<i16>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct FoFQuery {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct TargetsQuery {
    #[serde(default)]
    pub ids: Option<Value>,
    #[serde(default)]
    pub usernames: Option<Value>,
    #[serde(default)]
    pub metadata: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn user() -> UserResponse {
        UserResponse {
            id: "u".to_owned(),
            username: "bob".to_owned(),
            display_name: None,
            avatar_url: None,
            lang_tag: None,
            location: None,
            timezone: None,
            metadata: "{}".to_owned(),
            edge_count: None,
            create_time: "t".to_owned(),
            update_time: "t".to_owned(),
        }
    }

    #[test]
    fn a_friend_is_the_user_then_the_edge() {
        let t = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let friend = FriendResponse::new(user(), 0, t, &json!({}));
        assert_eq!(
            serde_json::to_string(&friend).unwrap(),
            r#"{"user":{"id":"u","username":"bob","metadata":"{}","create_time":"t","update_time":"t"},"state":0,"update_time":"2023-11-14T22:13:20Z","metadata":"{}"}"#
        );
    }

    #[test]
    fn an_absent_referrer_is_omitted_from_a_friends_of_friends_entry() {
        let entry = FriendsOfFriendsEntry {
            referrer: None,
            user: user(),
        };
        assert_eq!(
            serde_json::to_string(&entry).unwrap(),
            r#"{"user":{"id":"u","username":"bob","metadata":"{}","create_time":"t","update_time":"t"}}"#
        );
    }
}
