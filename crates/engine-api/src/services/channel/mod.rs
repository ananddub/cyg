use crate::services::plane::Plane;
use auto_di::singleton;
use std::sync::Arc;

pub mod membership;
pub mod messages;

pub struct ChannelService {
    pub(crate) plane: Arc<Plane>,
}

#[singleton]
impl ChannelService {
    pub fn new(plane: Arc<Plane>) -> Self {
        Self { plane }
    }
}
