use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::db::time;
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

#[derive(Debug, Clone, Deserialize)]
pub struct WriteResult {
    pub write: i16,
    pub version: String,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
    #[serde(default)]
    pub applied: bool,
}

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
