//! RFC 6455 framing and the handshake crypto it needs — self-contained, no
//! WebSocket crate.

mod crypto;
mod decode;
mod encode;

pub(super) use crypto::handle_ws_handshake;
pub(super) use decode::{MAX_INBOUND_FRAME_SIZE, next_ws_frame};
pub(super) use encode::{
    encode_wire_frame, encode_ws_frame, finish_close, queue_close_frame, queue_message,
    queue_ws_control, send_ws_ping,
};
