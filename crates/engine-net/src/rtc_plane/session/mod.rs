//! # Per-client WebRTC session (sans-io pump over `str0m::Rtc`)
//!
//! One [`RtcSession`] owns exactly one `str0m::Rtc` for one browser peer and
//! implements the canonical str0m run loop the async task in [`crate::rtc_plane`]
//! drives:
//!
//! ```text
//! poll_timeout -> sleep_until -> handle_input(timeout | datagram) -> drain poll_output
//! ```
//!
//! str0m 0.24 has no `Net<Server>` type; the sans-io engine *is* `Rtc`, so this
//! module wraps `Rtc` and presents the [`Outbound`] surface the plane needs:
//! [`Outbound::Datagram`] (a UDP write) and [`Outbound::Data`] (received
//! DataChannel payload). Every call that mutates `Rtc` here immediately drains
//! `poll_output` to the next [`str0m::Output::Timeout`], upholding str0m's
//! single-mutation invariant (mutate → drain → mutate).

mod engine;
mod outbound;
mod pump;
mod types;

pub use engine::RtcSession;
pub use types::{Outbound, PlaneChannel, SessionEvent};

/// Label contract: SCTP-unordered, 0-retransmits high-rate input / delta channel.
pub const LABEL_UNRELIABLE: &str = "game-unreliable";

/// Label contract: ordered, reliable channel for critical events.
pub const LABEL_RELIABLE: &str = "game-reliable";
