//! The database handle the whole account plane shares.

use std::sync::Arc;

use crate::ApiState;
use crate::error::V2Error;
use crate::repositories::db::Queries;

/// The account plane's query set, or 503 when `[control.session] database_url` is empty
/// — the one place the plane's dependence on the database becomes an error.
pub fn q(state: &ApiState) -> Result<Arc<Queries>, V2Error> {
    state.q.clone().ok_or_else(V2Error::no_db)
}
