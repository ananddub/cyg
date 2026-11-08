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

    LegacyAlias(Lane),

    Ignored,
}

pub fn classify_label(label: &str) -> LabelVerdict {
    match label {
        LABEL_UNRELIABLE => return LabelVerdict::Contract(Lane::Unreliable),
        LABEL_RELIABLE => return LabelVerdict::Contract(Lane::Reliable),
        _ => {}
    }
    if ends_with_token(label, "unreliable") {
        LabelVerdict::LegacyAlias(Lane::Unreliable)
    } else if ends_with_token(label, "reliable") {
        LabelVerdict::LegacyAlias(Lane::Reliable)
    } else {
        LabelVerdict::Ignored
    }
}

fn ends_with_token(label: &str, token: &str) -> bool {
    if label == token {
        return true;
    }
    match label.strip_suffix(token) {
        Some(rest) => matches!(rest.chars().next_back(), Some('-') | Some('_')),
        None => false,
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
