use engine_bus::{FrameKind, OutFrame};
use engine_webrtc_protocol::{MAX_DELTA_CHUNK, delta_fragment_count, write_delta_frame};

pub fn wrap_body(your_id: i64, guest_body: &[u8], acked_seq: u16, tick: u32, out: &mut Vec<u8>) {
    out.clear();
    out.reserve(guest_body.len() + 16);
    out.extend_from_slice(&your_id.to_le_bytes());
    out.extend_from_slice(guest_body);
    out.extend_from_slice(&acked_seq.to_le_bytes());
    out.extend_from_slice(&tick.to_le_bytes());
}

pub fn emit_delta(
    peer_id: &str,
    your_id: i64,
    guest_body: &[u8],
    acked_seq: u16,
    tick: u32,
    seq: u16,
    scratch: &mut Vec<u8>,
    out: &mut Vec<OutFrame>,
) -> Option<usize> {
    wrap_body(your_id, guest_body, acked_seq, tick, scratch);
    let total = delta_fragment_count(scratch.len())?;
    let before = out.len();
    for (idx, chunk) in scratch.chunks(MAX_DELTA_CHUNK).enumerate() {
        let mut frame = Vec::new();
        write_delta_frame(&mut frame, seq, tick, idx as u8, total, chunk);
        out.push(OutFrame {
            peer_id: peer_id.to_string(),
            kind: FrameKind::Unreliable,
            bytes: frame,
        });
    }
    Some(out.len() - before)
}

#[cfg(test)]
mod tests;
