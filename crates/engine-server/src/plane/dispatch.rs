//! Delta framing for both planes, owned by Agent 3C.
//!
//! One function builds the *whole* outbound shape of a tick delta:
//! body tail (`acked_seq u16 + server_tick u32` after the guest bytes, same
//! tail the WS plane appends in `src/server.rs::wrap_delta_into`, minus the
//! msgpack `"delta"` header), then channel-layer fragmentation
//! (`engine_webrtc_protocol::write_delta_frame`: `[0x04][seq u16][tick u32]
//! [frag_idx u8][frag_total u8][chunk ≤ MAX_DELTA_CHUNK]`).

use engine_bus::{FrameKind, OutFrame};
use engine_webrtc_protocol::{MAX_DELTA_CHUNK, delta_fragment_count, write_delta_frame};

/// Append `[your_id i64][guest body][acked_seq u16][tick u32]` into `out`.
/// (The guest body starts with its own version byte at index 8 of the final
/// buffer — the SDK's `_processDeltaPayload` tail guard depends on this.)
pub fn wrap_body(your_id: i64, guest_body: &[u8], acked_seq: u16, tick: u32, out: &mut Vec<u8>) {
    out.clear();
    out.reserve(guest_body.len() + 16);
    out.extend_from_slice(&your_id.to_le_bytes());
    out.extend_from_slice(guest_body);
    out.extend_from_slice(&acked_seq.to_le_bytes());
    out.extend_from_slice(&tick.to_le_bytes());
}

/// Stamp + fragment `body` (already AOI-filtered) for one peer. Returns the
/// number of `OutFrame`s pushed, or None if the body exceeds MAX_DELTA_BODY.
/// All frames carry `FrameKind::Unreliable` and the room-wide `seq`.
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
        // write_delta_frame clears its &mut Vec and builds the whole frame,
        // so the per-chunk allocation *is* the OutFrame bytes: no extra copy.
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
mod tests {
    use super::*;
    use engine_webrtc_protocol::{DeltaHeader, MAX_DELTA_BODY};

    fn sample_body(seed: u8, len: usize) -> Vec<u8> {
        // Byte 0 is the guest TLV version byte the SDK checks at offset 8.
        let mut v: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_add(seed)).collect();
        if !v.is_empty() {
            v[0] = 1;
        }
        v
    }

    #[test]
    fn wrap_body_layout() {
        let mut out = Vec::new();
        let body = [1u8, 2, 3, 4];
        wrap_body(-7, &body, 0x1122, 0x33445566, &mut out);
        // Contract tail is [acked u16][tick u32] = 6 bytes (same as server.rs
        // wrap_delta_into); the SDK's -8 read offset is a known Phase D defect.
        assert_eq!(out.len(), 8 + body.len() + 6);
        assert_eq!(&out[..8], &(-7i64).to_le_bytes());
        assert_eq!(&out[8..12], &body[..]);
        // SDK guard: TLV version byte lands at index 8 of the body buffer.
        assert_eq!(out[8], 1);
        assert_eq!(&out[out.len() - 6..out.len() - 4], &0x1122u16.to_le_bytes());
        assert_eq!(&out[out.len() - 4..], &0x33445566u32.to_le_bytes());
    }

    #[test]
    fn wrap_body_clears_first() {
        let mut out = vec![9u8; 50];
        wrap_body(1, &[1], 0, 0, &mut out);
        assert_eq!(out.len(), 15);
    }

    #[test]
    fn single_fragment_roundtrip() {
        let body = sample_body(3, 100);
        let mut scratch = Vec::new();
        let mut frames = Vec::new();
        let n = emit_delta("peer-a", 42, &body, 7, 999, 5, &mut scratch, &mut frames);
        assert_eq!(n, Some(1));
        let (h, chunk) = DeltaHeader::parse(&frames[0].bytes).unwrap();
        assert_eq!((h.seq, h.tick, h.frag_idx, h.frag_total), (5, 999, 0, 1));
        // One chunk carries the whole wrapped body: [id][guest][acked][tick].
        assert_eq!(chunk.len(), 8 + body.len() + 6);
        assert_eq!(&chunk[..8], &42i64.to_le_bytes());
        assert_eq!(&chunk[8..8 + body.len()], &body[..]);
        assert_eq!(&chunk[chunk.len() - 6..chunk.len() - 4], &7u16.to_le_bytes());
        assert_eq!(&chunk[chunk.len() - 4..], &999u32.to_le_bytes());
        assert_eq!(frames[0].peer_id, "peer-a");
        assert_eq!(frames[0].kind, FrameKind::Unreliable);
    }

    #[test]
    fn fragmented_reassembles() {
        let body = sample_body(5, MAX_DELTA_CHUNK * 2 + 37); // > 2 chunks of wrapped body
        let mut scratch = Vec::new();
        let mut frames = Vec::new();
        let n = emit_delta("p", -1, &body, 0xff00, 0x0bad_c0de, 0x1234, &mut scratch, &mut frames)
            .expect("fits");
        assert_eq!(n, frames.len());
        assert!(n >= 3);

        let mut reassembled = Vec::new();
        for (i, f) in frames.iter().enumerate() {
            let (h, chunk) = DeltaHeader::parse(&f.bytes).unwrap();
            assert_eq!(h.seq, 0x1234);
            assert_eq!(h.tick, 0x0bad_c0de);
            assert_eq!(h.frag_idx as usize, i);
            assert_eq!(h.frag_total as usize, n);
            reassembled.extend_from_slice(chunk);
        }
        // Reassembled stream is the wrapped body: [your_id][guest][acked][tick].
        assert_eq!(reassembled.len(), 8 + body.len() + 6);
        assert_eq!(&reassembled[..8], &(-1i64).to_le_bytes());
        assert_eq!(&reassembled[8..8 + body.len()], &body[..]);
        assert_eq!(&reassembled[reassembled.len() - 6..reassembled.len() - 4], &0xff00u16.to_le_bytes());
        assert_eq!(&reassembled[reassembled.len() - 4..], &0x0bad_c0de_u32.to_le_bytes());
    }

    #[test]
    fn oversized_returns_none() {
        let body = vec![0u8; MAX_DELTA_BODY]; // +14-byte wrap puts it over the cap
        let mut scratch = Vec::new();
        let mut frames = Vec::new();
        assert_eq!(emit_delta("p", 0, &body, 0, 0, 0, &mut scratch, &mut frames), None);
        assert!(frames.is_empty());
    }

    #[test]
    fn empty_body_still_emits_one_frame() {
        let mut scratch = Vec::new();
        let mut frames = Vec::new();
        assert_eq!(emit_delta("p", 0, &[], 0, 0, 1, &mut scratch, &mut frames), Some(1));
    }
}
