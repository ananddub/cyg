//! Storage helpers: the version hash, the row types and the checks run before a statement.

use chrono::{DateTime, Utc};
use md5::{Digest, Md5};
use uuid::Uuid;

use crate::dto::storage::{ObjectId, WriteObject};
use crate::error::V2Error;

pub const PERMIT_OWNER: i16 = 1;
pub const PERMIT_PUBLIC: i16 = 2;

pub const MAX_LIST_LIMIT: i64 = 100;
pub const LIST_LIMIT_MESSAGE: &str = "Invalid limit - limit must be between 1 and 100.";
pub const INVALID_USER_MESSAGE: &str = "Invalid user ID - make sure user ID is a valid UUID.";
pub const INVALID_OBJECT_MESSAGE: &str =
    "Invalid collection or key value supplied. They must be set.";
pub const INVALID_VALUE_MESSAGE: &str = "Value must be a JSON object.";
pub const INVALID_READ_MESSAGE: &str =
    "Invalid Read permission supplied. It must be either 0, 1 or 2.";
pub const INVALID_WRITE_MESSAGE: &str =
    "Invalid Write permission supplied. It must be either 0 or 1.";
pub const WRITE_REJECTED_MESSAGE: &str = "Storage write rejected.";
pub const DELETE_REJECTED_MESSAGE: &str = "Storage delete rejected.";
pub const CURSOR_MESSAGE: &str = "Malformed cursor was used.";

/// the `expectedVersion`: the MD5 hex of the value bytes as they arrived.
pub fn version_of(value: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StorageRow {
    pub collection: String,
    pub key: String,
    pub user_id: Uuid,
    pub value: String,
    pub version: String,
    pub read: i16,
    pub write: i16,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
}

/// A row's state after a write statement ran, whether or not it changed it.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WriteResult {
    pub write: i16,
    pub version: String,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
    pub applied: bool,
}

/// A write's value must be a JSON object and its permissions in range.
pub fn check_write(object: &WriteObject) -> Result<(), V2Error> {
    if object.collection.is_empty() || object.key.is_empty() || object.value.is_empty() {
        return Err(V2Error::invalid_argument(INVALID_OBJECT_MESSAGE));
    }
    if object
        .permission_read
        .is_some_and(|p| !(0..=PERMIT_PUBLIC as i32).contains(&p))
    {
        return Err(V2Error::invalid_argument(INVALID_READ_MESSAGE));
    }
    if object
        .permission_write
        .is_some_and(|p| !(0..=PERMIT_OWNER as i32).contains(&p))
    {
        return Err(V2Error::invalid_argument(INVALID_WRITE_MESSAGE));
    }
    if !object.value.trim_start().starts_with('{')
        || serde_json::from_str::<serde_json::Value>(&object.value).is_err()
    {
        return Err(V2Error::invalid_argument(INVALID_VALUE_MESSAGE));
    }
    Ok(())
}

pub fn check_id(id: &ObjectId) -> Result<(), V2Error> {
    if id.collection.is_empty() || id.key.is_empty() {
        return Err(V2Error::invalid_argument(INVALID_OBJECT_MESSAGE));
    }
    Ok(())
}

/// The owner an object id names; absent means the caller's own account, which is
/// what the SDKs send for "my objects". Spelling out the nil UUID is refused, as BaaS does.
pub fn read_owner(id: &ObjectId, caller: Uuid) -> Result<Uuid, V2Error> {
    if id.user_id.is_empty() {
        return Ok(caller);
    }
    let owner = Uuid::parse_str(&id.user_id)
        .map_err(|_| V2Error::invalid_argument(INVALID_USER_MESSAGE))?;
    if owner == Uuid::nil() {
        return Err(V2Error::invalid_argument(INVALID_USER_MESSAGE));
    }
    Ok(owner)
}
