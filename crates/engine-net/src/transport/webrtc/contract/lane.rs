use str0m::channel::{ChannelConfig, Reliability};

use super::{LABEL_RELIABLE, LABEL_UNRELIABLE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lane {
    Unreliable,

    Reliable,
}

impl Lane {
    pub fn label(self) -> &'static str {
        match self {
            Lane::Unreliable => LABEL_UNRELIABLE,
            Lane::Reliable => LABEL_RELIABLE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelVerdict {
    Contract(Lane),

    Ignored,
}

/// Only the two contract labels are data lanes. Any other label is ignored —
/// there is no suffix/alias matching, so a typo or an old SDK cannot silently
/// bind gameplay traffic to the wrong lane.
pub fn classify_label(label: &str) -> LabelVerdict {
    match label {
        LABEL_UNRELIABLE => LabelVerdict::Contract(Lane::Unreliable),
        LABEL_RELIABLE => LabelVerdict::Contract(Lane::Reliable),
        _ => LabelVerdict::Ignored,
    }
}

pub fn contract_mismatch(lane: Lane, config: &ChannelConfig) -> Option<&'static str> {
    match lane {
        Lane::Unreliable => {
            if config.ordered {
                return Some("game-unreliable channel was opened ordered");
            }
            if !matches!(
                config.reliability,
                Reliability::MaxRetransmits { retransmits: 0 }
            ) {
                return Some("game-unreliable channel allows retransmits");
            }
        }
        Lane::Reliable => {
            if !config.ordered {
                return Some("game-reliable channel was opened unordered");
            }
            if !matches!(config.reliability, Reliability::Reliable) {
                return Some("game-reliable channel can drop messages");
            }
        }
    }
    None
}
