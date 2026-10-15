//! Wire-frame builders shared by the contract test modules.

use engine_webrtc_protocol::write_delta_frame;

/// One stamped delta frame as it would appear on the wire.
pub(super) fn frame(seq: u16, tick: u32, idx: u8, total: u8, chunk: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    write_delta_frame(&mut out, seq, tick, idx, total, chunk);
    out
}
