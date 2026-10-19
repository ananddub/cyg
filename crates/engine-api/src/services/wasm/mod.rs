//! Dynamic WASM room modules: the store behind
//! `/v2/console/games/{gameId}/wasm*`.
//!
//! Owns the `room_modules` table — hand-written sqlx, as every console area
//! does — plus the ABI gate an upload must pass before it is stored, and the
//! [`engine_bus::WasmReload`] publish that turns an activation into a live
//! hot-swap on the sim plane.

pub mod upload;

pub use upload::{WasmService, REQUIRED_EXPORTS};

use crate::error::V2Error;
use crate::ApiState;

/// The pool `room_modules` lives on; 503 when the database is not configured.
pub(super) fn pool(state: &ApiState) -> Result<&sqlx::PgPool, V2Error> {
    state.db.as_ref().ok_or_else(V2Error::no_db)
}
