use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListUserEdgesByStateRow {
    pub id: Uuid,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub metadata: serde_json::Value,
    pub lang_tag: String,
    pub location: Option<String>,
    pub timezone: Option<String>,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
    pub state: Option<i16>,
    pub edge_update_time: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListFriendsOfFriendsRow {
    pub referrer: Option<Uuid>,
    pub id: Option<Uuid>,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub metadata: serde_json::Value,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListUserEdgesPagedRow {
    pub id: Uuid,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub metadata: serde_json::Value,
    pub lang_tag: String,
    pub location: Option<String>,
    pub timezone: Option<String>,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
    pub state: Option<i16>,
    pub edge_position: Option<i64>,
    pub edge_update_time: Option<DateTime<Utc>>,
    pub edge_metadata: Option<serde_json::Value>,
}

pub type ListUserEdgesByStatePagedRow = ListUserEdgesPagedRow;

pub struct AddFriendEdgeParams {
    pub source_id: Uuid,
    pub destination_id: Uuid,
    pub state: i16,
    pub position: i64,
}

pub struct BlockUserEdgesParams {
    pub source_id: Uuid,
    pub destination_id: Uuid,
    pub position: i64,
}

pub struct ListUserEdgesByStateParams {
    pub source_id: Uuid,
    pub state: i16,
    pub limit: i64,
}

pub struct ListFriendsOfFriendsParams {
    pub source_id: Uuid,
    pub source_id2: Uuid,
    pub destination_id_start: Uuid,
    pub limit: i64,
}

pub struct UpdateFriendEdgeMetadataParams {
    pub metadata: serde_json::Value,
    pub source_id: Uuid,
    pub destination_id: Uuid,
}

pub struct InsertFriendEdgeParams {
    pub source_id: Uuid,
    pub destination_id: Uuid,
    pub state: i16,
    pub position: i64,
    pub metadata: serde_json::Value,
}

pub struct ListUserEdgesPagedParams {
    pub source_id: Uuid,
    pub state: i16,
    pub position: i64,
    pub limit: i64,
}

pub type ListUserEdgesByStatePagedParams = ListUserEdgesPagedParams;

pub struct IncrementPairEdgeCountParams {
    pub id: Uuid,
    pub id2: Uuid,
    pub position: i64,
}
