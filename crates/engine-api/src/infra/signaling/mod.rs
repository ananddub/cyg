//! WebRTC signaling: the frame types and routing hub now live in the
//! `engine-signaling` crate (shared with the room host, which used to
//! depend on this whole HTTP crate for four types). The axum-facing WS glue
//! (socket + inbound handling + wire encoding) stays here.

mod inbound;
pub(crate) mod socket;
mod wire;

pub use engine_signaling::{
    INBOUND_QUEUE, PER_SEAT_QUEUE, SeatReplies, SignalHub, SignalIn, SignalOut,
};
