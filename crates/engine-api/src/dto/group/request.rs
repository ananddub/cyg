//! The group request shapes.

use serde::Deserialize;
use serde_json::Value;

/// `POST /v2/group` body — the group shape. `metadata` is a JSON string on the wire,
/// and `open` defaults to false.
#[derive(Debug, Default, Deserialize)]
pub struct CreateGroupRequest {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub creator_id: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub lang_tag: Option<String>,
    #[serde(default)]
    pub metadata: Option<String>,
    #[serde(default)]
    pub open: Option<bool>,
    #[serde(default)]
    pub max_count: Option<i32>,
}

/// `PUT /v2/group/{group_id}` body — `apiUpdateGroup`.
#[derive(Debug, Default, Deserialize)]
pub struct UpdateGroupRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub lang_tag: Option<String>,
    #[serde(default)]
    pub metadata: Option<String>,
    #[serde(default)]
    pub max_count: Option<i32>,
    #[serde(default)]
    pub open: Option<bool>,
}

/// The paging query shared by the three group list surfaces.
#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub state: Option<i16>,
    #[serde(default, alias = "langTag")]
    pub lang_tag: Option<String>,
    /// A nullable boolean in the query string, so every spelling
    /// `strconv.ParseBool` takes has to work here too.
    #[serde(default)]
    pub open: Option<String>,
    #[serde(default)]
    pub members: Option<i64>,
}

/// `POST /v2/group/{group_id}/add|kick|ban|promote|demote`: `?user_ids=` / `?userIds=`,
/// the gateway filling a repeated field from either spelling of the name.
#[derive(Debug, Default, Deserialize)]
pub struct UsersQuery {
    #[serde(default, alias = "userIds")]
    pub user_ids: Option<Value>,
}
