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
