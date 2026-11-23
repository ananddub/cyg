use crate::services::plane::Plane;
use auto_di::singleton;
use std::sync::Arc;

pub mod outbox;
pub(crate) mod preferences;
pub(crate) mod worker;

pub use worker::delivery_loop;

pub struct NotifyService {
    pub(crate) plane: Arc<Plane>,
}

#[singleton]
impl NotifyService {
    pub fn new(plane: Arc<Plane>) -> Self {
        Self { plane }
    }
}
