//! Storage engine (`/v2/storage/*`).
//!
//! BaaS parity: `server/core_storage.go`, `server/api_storage.go`. Objects are keyed by
//! `(collection, key, user_id)` and a write is a compare-and-set on the MD5 hex version.

use auto_di::singleton;

use crate::ApiState;
use crate::error::V2Error;

pub mod delete;
pub mod index;
pub mod list;
pub mod read;
pub mod write;

pub struct StorageService;

#[singleton]
impl StorageService {
    fn new() -> Self {
        Self
    }
}

/// The pool storage runs against, or 503 when `[control.session] database_url` is empty. The
/// statements are hand-written raw sqlx, so they take the pool, not the wrapper.
pub(crate) fn pool(state: &ApiState) -> Result<&sqlx::PgPool, V2Error> {
    state.db.as_ref().ok_or_else(V2Error::no_db)
}
