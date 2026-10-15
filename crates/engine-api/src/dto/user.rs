//! `apiUser` — the public view of an account, shared by the social reads and the
//! `/v2/user` lookups.

use std::borrow::Cow;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::repositories::models::Users;
use crate::services::social::{meta_str, ts, uid};

/// The user columns a join may expose. [`UserResponse`] is built from this, so an edge
/// listing and an account read emit the same shape — and neither can leak `wallet`/`email`.
pub struct UserCols<'a> {
    pub id: Cow<'a, str>,
    pub username: &'a str,
    pub display_name: Option<&'a str>,
    pub avatar_url: Option<&'a str>,
    pub lang_tag: &'a str,
    pub location: Option<&'a str>,
    pub timezone: Option<&'a str>,
    pub metadata: &'a Value,
    pub edge_count: Option<i32>,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
}

impl<'a> From<&'a Users> for UserCols<'a> {
    fn from(user: &'a Users) -> UserCols<'a> {
        UserCols {
            id: Cow::Owned(uid(&user.id)),
            username: &user.username,
            display_name: user.display_name.as_deref(),
            avatar_url: user.avatar_url.as_deref(),
            lang_tag: &user.lang_tag,
            location: user.location.as_deref(),
            timezone: user.timezone.as_deref(),
            metadata: &user.metadata,
            edge_count: Some(user.edge_count),
            create_time: user.create_time,
            update_time: user.update_time,
        }
    }
}

/// The public user shape. The gateway omits default-valued members, so an
/// empty string or a zero count is absent rather than `""` / `0`.
#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: String,
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    /// A JSON string, because the wire type is a `string`: an object breaks stock SDKs.
    pub metadata: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_count: Option<i32>,
    pub create_time: String,
    pub update_time: String,
}

impl UserResponse {
    pub fn from_cols(cols: &UserCols) -> Self {
        UserResponse {
            id: cols.id.as_ref().to_owned(),
            username: cols.username.to_owned(),
            display_name: non_empty(cols.display_name),
            avatar_url: non_empty(cols.avatar_url),
            lang_tag: non_empty(Some(cols.lang_tag)),
            location: non_empty(cols.location),
            timezone: non_empty(cols.timezone),
            metadata: meta_str(cols.metadata),
            edge_count: cols.edge_count.filter(|count| *count != 0),
            create_time: ts(cols.create_time),
            update_time: ts(cols.update_time),
        }
    }

    /// The view of a stored account, as the `/v2/user` lookups emit it.
    pub fn from_row(user: &Users) -> Self {
        UserResponse::from_cols(&UserCols::from(user))
    }
}

/// `""` is dropped from the wire too, so it folds into "absent".
fn non_empty(value: Option<&str>) -> Option<String> {
    value.filter(|s| !s.is_empty()).map(str::to_owned)
}

/// `GetUsers`: the rows that were found, in the order they were requested.
#[derive(Debug, Serialize)]
pub struct UserListResponse {
    pub users: Vec<UserResponse>,
}

/// `GET /v2/user` query: every multi-get key is a repeated field, which the gateway may
/// deliver as repeated params, a JSON array or a comma-joined string.
#[derive(Debug, Default, Deserialize)]
pub struct GetUsersQuery {
    #[serde(default)]
    pub ids: Option<Value>,
    #[serde(default)]
    pub usernames: Option<Value>,
    #[serde(default)]
    pub facebook_ids: Option<Value>,
    #[serde(default)]
    pub custom_ids: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn at(secs: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(secs, 0).unwrap()
    }

    #[test]
    fn an_empty_string_or_zero_count_is_absent() {
        let cols = UserCols {
            id: Cow::Owned("00000000-0000-0000-0000-000000000001".to_owned()),
            username: "bob",
            display_name: None,
            avatar_url: Some("http://a"),
            lang_tag: "",
            location: None,
            timezone: Some("UTC"),
            metadata: &json!({}),
            edge_count: Some(0),
            create_time: at(1_700_000_000),
            update_time: at(1_700_000_000),
        };
        assert_eq!(
            serde_json::to_string(&UserResponse::from_cols(&cols)).unwrap(),
            concat!(
                r#"{"id":"00000000-0000-0000-0000-000000000001","username":"bob","#,
                r#""avatar_url":"http://a","timezone":"UTC","metadata":"{}","#,
                r#""create_time":"2023-11-14T22:13:20Z","update_time":"2023-11-14T22:13:20Z"}"#,
            )
        );
    }
}
