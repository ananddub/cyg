use str0m::channel::{ChannelConfig, Reliability};

use super::{LABEL_RELIABLE, LABEL_UNRELIABLE};

pub(super) fn unreliable_config() -> ChannelConfig {
    ChannelConfig {
        label: LABEL_UNRELIABLE.to_string(),
        ordered: false,
        reliability: Reliability::MaxRetransmits { retransmits: 0 },
        ..Default::default()
    }
}

pub(super) fn reliable_config() -> ChannelConfig {
    ChannelConfig {
        label: LABEL_RELIABLE.to_string(),
        ordered: true,
        reliability: Reliability::Reliable,
        ..Default::default()
    }
}
