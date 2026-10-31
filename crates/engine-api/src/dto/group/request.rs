use serde::Deserialize;
use serde_json::Value;

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
    #[serde(default)]
    pub open: Option<String>,
    #[serde(default)]
    pub members: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UsersQuery {
    #[serde(default, alias = "userIds")]
    pub user_ids: Option<Value>,
}
