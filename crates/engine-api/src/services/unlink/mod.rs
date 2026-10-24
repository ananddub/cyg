//! Detaching a credential from the signed-in account (`/v2/account/unlink/*`).
//!
//! An unlink is *guarded*, not idempotent: each statement clears its column only while the
//! account keeps another way in. A non-empty body credential adds `AND <column> = $2`.

use auto_di::singleton;
use axum::http::HeaderMap;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::session_claims;
use crate::error::V2Error;

pub mod credential;
pub mod device;
pub mod token;

pub struct UnlinkService;

#[singleton]
impl UnlinkService {
    fn new() -> Self {
        Self
    }
}

/// The account the credential is detached from.
pub(crate) fn bearer(headers: &HeaderMap, state: &ApiState) -> Result<Uuid, V2Error> {
    session_claims(headers, state).and_then(|claims| claims.uuid())
}
