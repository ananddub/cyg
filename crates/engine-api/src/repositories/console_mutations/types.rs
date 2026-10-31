use super::*;

pub type FindConsoleUserByIdRow = FindConsoleUserRow;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FindConsoleUserMfaRow {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub acl: serde_json::Value,
    #[serde(with = "crate::db::bytes::b64_opt")]
    pub password: Option<Vec<u8>>,
    pub disable_time: DateTime<Utc>,
    #[serde(with = "crate::db::bytes::b64_opt")]
    pub mfa_secret: Option<Vec<u8>>,
    #[serde(with = "crate::db::bytes::b64_opt")]
    pub mfa_recovery_codes: Option<Vec<u8>>,
    pub mfa_required: Option<bool>,
}

pub type FindConsoleUserMfaByIdRow = FindConsoleUserMfaRow;

pub type ListStorageForUserRow = ListStorageRow;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ListUserEdgesRow {
    pub destination_id: Uuid,
    pub state: i16,
    pub position: i64,
    pub metadata: serde_json::Value,
}

pub(crate) use crate::repositories::db::{WriteResult, affected};

pub struct UpsertConsoleUserParams {
    pub id: Uuid,
    pub email: String,
    pub acl: serde_json::Value,
    pub username: String,
    pub password: Vec<u8>,
    pub mfa_required: bool,
}

pub struct InsertConsoleUserParams {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub acl: serde_json::Value,
    pub password: Vec<u8>,
    pub mfa_required: bool,
}

pub struct UpdateConsoleUserParams {
    pub username: String,
    pub email: String,
    pub acl: serde_json::Value,
    pub metadata: serde_json::Value,
    pub mfa_required: bool,
    pub id: Uuid,
}

pub struct SetConsoleUserMfaParams {
    pub mfa_secret: Vec<u8>,
    pub mfa_recovery_codes: Vec<u8>,
    pub mfa_required: bool,
    pub id: Uuid,
}

pub struct ImportDeviceParams {
    pub id: String,
    pub provider: String,
}

pub struct ImportNoteParams {
    pub id: Uuid,
    pub note: String,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
}

pub struct ImportLedgerParams {
    pub id: Uuid,
    pub changeset: serde_json::Value,
    pub metadata: serde_json::Value,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
}

pub struct ImportAccountParams {
    pub account: Users,
    pub devices: Vec<ImportDeviceParams>,
    pub notes: Vec<ImportNoteParams>,
    pub wallet_ledger: Vec<ImportLedgerParams>,
    pub storage: Vec<ListStorageRow>,
    pub edges: Vec<ListUserEdgesRow>,
}

pub struct InsertUserNoteParams {
    pub id: Uuid,
    pub user_id: Uuid,
    pub note: String,
    pub create_id: Uuid,
}

pub struct InsertUserNoteAnonymousParams {
    pub id: Uuid,
    pub user_id: Uuid,
    pub note: String,
}

pub struct InsertNotificationParams {
    pub id: Uuid,
    pub user_id: Uuid,
    pub subject: String,
    pub content: serde_json::Value,
    pub code: i16,
    pub sender_id: Uuid,
}

pub struct InsertAclTemplateParams {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub acl: serde_json::Value,
}

pub struct UpdateAclTemplateParams {
    pub name: String,
    pub description: String,
    pub acl: serde_json::Value,
    pub id: Uuid,
}

pub struct ListAuditLogsFilteredParams {
    pub action: String,
    pub resource: String,
    pub console_username: String,
    pub create_time_start: DateTime<Utc>,
    pub create_time_end: DateTime<Utc>,
    pub limit: i64,
    pub offset: i64,
}

pub struct InsertAuditLogParams {
    pub id: Uuid,
    pub console_user_id: Uuid,
    pub console_username: String,
    pub email: String,
    pub action: String,
    pub resource: String,
    pub message: String,
    pub metadata: serde_json::Value,
}

pub(super) const UPSERT_CONSOLE_USER: &str = "UPDATE type::record('console_user', <uuid>$id) SET \
    update_time = $now, acl = $acl, username = $username, password = $password, \
    mfa_required = $mfa_required RETURN AFTER";

pub(super) fn upsert_console_user_params(p: &UpsertConsoleUserParams) -> BTreeMap<String, Value> {
    params!(
        id = p.id.to_string(),
        acl = p.acl,
        username = p.username,
        password = bytes::encode(&p.password),
        mfa_required = p.mfa_required,
        now = time::encode(Utc::now()),
    )
}
