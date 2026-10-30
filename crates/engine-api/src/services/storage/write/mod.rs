use std::collections::BTreeMap;

use axum::Json;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::db::DbError;
use crate::dto::storage::{StorageObjectAck, StorageObjectAcks, WriteObject, WriteRequest};
use crate::error::V2Error;
use crate::repositories::storage::{
    StorageCurrentRow, storage_insert_statement, storage_update_statement, version_of,
};
use crate::utils::storage::{PERMIT_OWNER, WRITE_REJECTED_MESSAGE, WriteResult, check_write};

mod current;
mod update_statement;

use update_statement::*;
