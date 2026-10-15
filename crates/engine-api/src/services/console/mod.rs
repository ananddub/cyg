//! The admin console: its own authentication realm plus the read-only views an
//! operator browses.
//!
//! One service type with a module per area — the shape
//! [`crate::services::account`] uses — so a controller injects a single service
//! and reaches every area method, and an area can split once it grows.

use auto_di::singleton;

use crate::ApiState;
use crate::error::V2Error;
use crate::repositories::db::Queries;

pub mod account;
pub mod audit;
pub mod auth;
pub mod group;
pub mod iap;
pub mod leaderboard;
pub mod notification;
pub mod setting;
pub mod storage;
pub mod user;

pub struct ConsoleService;

#[singleton]
impl ConsoleService {
    fn new() -> Self {
        Self
    }
}

/// The console is entirely database-backed: without a pool every view answers 503
/// rather than pretending a table is empty.
pub(super) fn queries(state: &ApiState) -> Result<std::sync::Arc<Queries>, V2Error> {
    state.q.clone().ok_or_else(V2Error::no_db)
}

/// Parse a path id, answering the way the gateway does for a malformed one.
pub(super) fn account_id(raw: &str) -> Result<uuid::Uuid, V2Error> {
    uuid::Uuid::parse_str(raw).map_err(|_| V2Error::invalid_argument("Invalid account id."))
}
