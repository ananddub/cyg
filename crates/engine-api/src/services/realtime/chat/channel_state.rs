use super::*;
use std::sync::Arc;

use crate::services::plane::Plane;

pub(super) const ROLE_MEMBER: i16 = 2;
pub struct ChannelState {
    pub stream: Stream,
    pub members: HashMap<Uuid, Presence>,
}
pub struct RealtimeChatService {
    pub plane: Arc<Plane>,
    pub channels: FastMap<String, ChannelState>,
}
#[singleton]
impl RealtimeChatService {
    pub fn new(plane: Arc<Plane>) -> Self {
        Self {
            plane,
            channels: new_fast_map(),
        }
    }
}
