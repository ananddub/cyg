use crate::services::plane::Plane;
use auto_di::singleton;
use std::sync::Arc;

pub mod handshake;

pub struct SignalingService {
    pub(crate) plane: Arc<Plane>,
}

#[singleton]
impl SignalingService {
    pub fn new(plane: Arc<Plane>) -> Self {
        Self { plane }
    }
}
