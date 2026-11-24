use crate::services::plane::Plane;
use auto_di::singleton;
use axum::http::HeaderMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::app::middleware::session_claims;
use crate::error::V2Error;

pub mod credential;
pub mod device;
pub mod token;

pub struct UnlinkService {
    pub(crate) plane: Arc<Plane>,
}

#[singleton]
impl UnlinkService {
    pub fn new(plane: Arc<Plane>) -> Self {
        Self { plane }
    }
}

pub(crate) fn bearer(headers: &HeaderMap, plane: &Plane) -> Result<Uuid, V2Error> {
    session_claims(headers, plane).and_then(|claims| claims.uuid())
}
