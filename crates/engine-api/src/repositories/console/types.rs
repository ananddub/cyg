use super::*;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FindConsoleUserRow {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub acl: serde_json::Value,
    pub metadata: serde_json::Value,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
    pub disable_time: DateTime<Utc>,
    pub mfa_required: Option<bool>,
}

pub type ListConsoleUsersRow = FindConsoleUserRow;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ListStorageRow {
    pub collection: String,
    pub object_key: String,
    pub user_id: Uuid,
    pub value: String,
    pub version: String,
    pub read: i16,
    pub write: i16,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
}

pub type ListStorageInCollectionRow = ListStorageRow;

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct StoredObject {
    pub id: String,
    #[serde(default)]
    pub key: Option<String>,
    pub collection: String,
    pub user_id: Uuid,
    #[serde(default)]
    pub value: serde_json::Value,
    pub version: String,
    pub read: i16,
    pub write: i16,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
}

impl From<StoredObject> for ListStorageRow {
    fn from(row: StoredObject) -> Self {
        Self {
            collection: row.collection,
            object_key: row.key.unwrap_or(row.id),
            user_id: row.user_id,
            value: row.value.to_string(),
            version: row.version,
            read: row.read,
            write: row.write,
            create_time: row.create_time,
            update_time: row.update_time,
        }
    }
}

pub(crate) const STORAGE_COLUMNS: &str =
    "id, collection, key, user_id, value, version, read, write, create_time, update_time";

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct AuditUsernameCount {
    pub console_username: String,
    pub total: i64,
}

pub struct ListWalletLedgerParams {
    pub user_id: Uuid,
    pub limit: i64,
    pub offset: i64,
}

pub struct ListLeaderboardsByCategoryParams {
    pub category: i16,
    pub limit: i64,
    pub offset: i64,
}
