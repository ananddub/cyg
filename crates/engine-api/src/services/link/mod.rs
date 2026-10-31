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

pub(crate) fn bearer(headers: &HeaderMap, state: &ApiState) -> Result<Uuid, V2Error> {
    session_claims(headers, state).and_then(|claims| claims.uuid())
}
