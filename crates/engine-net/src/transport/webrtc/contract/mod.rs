//! Channel-contract policy for the browser `game-*` DataChannels.
//!
//! [`engine_webrtc_protocol::channel`] owns the *bytes* (header layout,
//! fragment sizing, ack codec). This module owns the *rules* the transport has
//! to apply on top of them: which channel is which lane, what is allowed to
//! arrive on it, how a delta becomes MTU-safe frames, and what happens when a
//! lane's send buffer is full.
//!
//! Everything here is pure — no `Rtc`, no sockets — so the contract is testable
//! without driving a DTLS handshake.

mod delta;
mod inbound;
mod lane;
mod outbound;
mod queue;
mod watermark;

#[cfg(test)]
mod fixture;
#[cfg(test)]
mod delta_tests;
#[cfg(test)]
mod inbound_tests;
#[cfg(test)]
mod lane_tests;
#[cfg(test)]
mod queue_tests;
#[cfg(test)]
mod watermark_tests;

pub use delta::{DeltaTooLarge, split_delta};
pub use inbound::{Inbound, RejectReason, classify_inbound, input_body};
pub use lane::{LabelVerdict, Lane, classify_label, contract_mismatch};
pub use outbound::OutboundDelta;
pub use queue::OutboundQueue;
pub use watermark::SeqWatermark;

use engine_webrtc_protocol::{
    HEADER_LEN, INPUT_WIRE_LEN, MAX_FRAGMENTS, MAX_FRAME,
};

/// The unreliable lane's channel label, matched exactly.
pub const LABEL_UNRELIABLE: &str = "game-unreliable";

/// The reliable lane's channel label, matched exactly.
pub const LABEL_RELIABLE: &str = "game-reliable";

/// Bytes of input-frame body after `[op][seq u16]`.
pub const INPUT_PAYLOAD_LEN: usize = INPUT_WIRE_LEN - HEADER_LEN;

/// A frame is only ever one DataChannel message, and one DataChannel message is
/// only useful if it fits the path MTU: SCTP fragmentation loses the *whole*
/// message when any chunk is lost and keeps reassembly state until it times out.
pub const FRAME_MAX_WIRE_LEN: usize = MAX_FRAME;

/// Frames a lane will hold for a client that is not draining. Sized so the
/// largest legal delta (`MAX_FRAGMENTS` chunks) always fits on its own, which
/// is what makes "drop the oldest whole delta" a rule we can actually keep.
pub const MAX_QUEUED_FRAMES: usize = MAX_FRAGMENTS + 1;
