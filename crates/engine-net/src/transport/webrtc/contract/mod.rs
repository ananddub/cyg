
mod delta;
mod inbound;
mod lane;
mod outbound;
mod queue;
mod watermark;

#[cfg(test)]
mod delta_tests;
#[cfg(test)]
mod fixture;
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

use engine_webrtc_protocol::{HEADER_LEN, INPUT_WIRE_LEN, MAX_FRAGMENTS, MAX_FRAME};

pub const LABEL_UNRELIABLE: &str = "game-unreliable";

pub const LABEL_RELIABLE: &str = "game-reliable";

pub const INPUT_PAYLOAD_LEN: usize = INPUT_WIRE_LEN - HEADER_LEN;

pub const FRAME_MAX_WIRE_LEN: usize = MAX_FRAME;

pub const MAX_QUEUED_FRAMES: usize = MAX_FRAGMENTS + 1;
