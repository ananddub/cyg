use super::*;

pub(super) const ROLE_MEMBER: i16 = 2;
pub struct ChannelState {
    pub stream: Stream,
    pub members: HashMap<Uuid , Presence>,
}
pub struct RealtimeChatService {
    pub channels: FastMap<String, ChannelState>,
}
#[singleton]
impl RealtimeChatService {
    pub fn new() -> Self {
        Self {
            channels: new_fast_map(),
        }
    }
}
