//! Which channel label is which lane, and whether the negotiated SCTP
//! parameters agree.

use str0m::channel::{ChannelConfig, Reliability};

use super::{LABEL_RELIABLE, LABEL_UNRELIABLE};

/// Which side of the reliability contract a channel serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lane {
    /// `{ ordered: false, maxRetransmits: 0 }`: inputs, and the server's deltas.
    Unreliable,
    /// `{ ordered: true }`: events and the acks that confirm them.
    Reliable,
}

impl Lane {
    /// The label this lane must carry for a channel to be part of the contract.
    pub fn label(self) -> &'static str {
        match self {
            Lane::Unreliable => LABEL_UNRELIABLE,
            Lane::Reliable => LABEL_RELIABLE,
        }
    }
}

/// What the transport decided about a channel label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelVerdict {
    /// Exact contract label.
    Contract(Lane),
    /// A lane-naming label from an older SDK bundle (for example a prefixed
    /// `client-unreliable`). Accepted, but warned about once, because it is not
    /// something the server should be relying on.
    LegacyAlias(Lane),
    /// Not part of the contract. Deliberately *not* defaulted to reliable: an
    /// unknown channel that silently joins the event lane is how a debug or
    /// telemetry channel ends up carrying gameplay.
    Ignored,
}

/// Match a channel label against the contract.
///
/// The exact labels win; anything else is only accepted if it *ends* with a
/// lane word as its own dash/underscore-separated token. The unreliable test
/// must run first because `"unreliable"` also ends with `"reliable"`.
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

/// The lane word must be its own dash/underscore-separated token at the end:
/// `client-reliable` counts, `fooreliable` and `x-reliable-y` do not.
fn ends_with_token(label: &str, token: &str) -> bool {
    if label == token {
        return true;
    }
    match label.strip_suffix(token) {
        Some(rest) => matches!(rest.chars().next_back(), Some('-') | Some('_')),
        None => false,
    }
}

/// Do the SCTP parameters the peer actually negotiated match the lane?
///
/// A label only names a lane; `ordered`/`reliability` are what the channel
/// *is*. Returning the mismatch instead of guessing lets the transport keep
/// serving an older bundle while still reporting that the reliability mapping
/// for that session is not what the contract promises.
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
