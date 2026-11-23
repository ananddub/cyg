use crate::services::plane::Plane;
use auto_di::singleton;
use std::sync::Arc;

pub mod entitlements;
pub mod purchases;
pub mod subscriptions;
pub mod vendors;
pub(crate) mod verify;

pub struct IapService {
    pub(crate) plane: Arc<Plane>,
}

#[singleton]
impl IapService {
    pub fn new(plane: Arc<Plane>) -> Self {
        Self { plane }
    }
}
