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

pub(crate) fn bearer(headers: &HeaderMap, state: &ApiState) -> Result<Uuid, V2Error> {
    session_claims(headers, state).and_then(|claims| claims.uuid())
}
