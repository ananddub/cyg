//! Attaching a credential to the signed-in account (`/v2/account/link/*`).
//!
//! Body-before-auth is required (the gateway marshals before its security step, so a
//! decode failure is 400 regardless of token), and a link never *moves* an identity.

use auto_di::singleton;
use axum::http::HeaderMap;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::session_claims;
use crate::error::V2Error;

pub mod local;
pub mod social;

pub struct LinkService;

#[singleton]
impl LinkService {
    fn new() -> Self {
        Self
    }
}

/// The account the credential is attached to. Every link route is bearer
/// authenticated: unlike authentication, a link proves who the caller already is.
pub(crate) fn bearer(headers: &HeaderMap, state: &ApiState) -> Result<Uuid, V2Error> {
    session_claims(headers, state).and_then(|claims| claims.uuid())
}
