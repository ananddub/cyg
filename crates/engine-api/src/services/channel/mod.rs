//! Channels (`/v2/channel/{channel_id}`): a message stream whose client id is the
//! dotted four-part `<mode>.<subject>.<descriptor>.<label>` (2=room, 3=group, 4=DM).
//! BaaS parity: `server/api_channel.go`, `core_channel.go`; the converters in
//! [`crate::utils::channel`] are shared with the realtime `channel_join` op.

use auto_di::singleton;

pub mod membership;
pub mod messages;

pub struct ChannelService;

#[singleton]
impl ChannelService {
    fn new() -> Self {
        Self
    }
}
