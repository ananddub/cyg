//! Raw room-data payload unwrapping: what the transport hands the client
//! before the OXS2 section starts.
//!
//! Server wire shape (see `crates/engine-server/src/netcode/schema.rs`
//! `split_schema` and cyg.js `_processDeltaPayload`):
//!
//! - optional delta-fragment header `[0x04][seq u16][tick u32][frag_idx u8]
//!   [frag_total u8][chunk]` — reassembled via the verified
//!   `engine-webrtc-protocol` codec,
//! - WS prefix `[0x11][0xA5 "delta"]` (7 bytes) or bare `[0xA5 "delta"]`
//!   (6 bytes),
//! - envelope `[your_id i64 LE][OXS2 section...][acked_seq u16][server_tick u32]`.

use engine_webrtc_protocol::channel::{DeltaHeader, MAX_DELTA_BODY};

/// What a payload decodes into once the wrappers are stripped.
///
/// `acked_seq` / `server_tick` come from the envelope tail
/// (`[acked_seq u16][server_tick u32]`) and drive client-side prediction:
/// `acked_seq` is the highest input sequence the server has processed
/// (the rollback ack), `server_tick` is the server's simulation tick.
pub struct RawFrame<'a> {
    pub section: &'a [u8],
    /// Highest input seq the server has confirmed (0 = none acked yet).
    pub acked_seq: u16,
    /// Server simulation tick at send time.
    pub server_tick: u32,
}

/// Reassemble a fragmented delta. Chunks carry the same
/// `[0x04][seq][tick][idx][total]` header the verified wire crate parses.
pub struct DeltaReassembler {
    buf: Vec<u8>,
}

impl DeltaReassembler {
    pub fn new() -> Self {
        Self {
            buf: Vec::with_capacity(4096),
        }
    }

    /// Feed one (possibly fragmented) payload. Returns `Some(assembled)` when
    /// the final fragment of a delta arrived, `None` while parts are pending.
    pub fn push(&mut self, payload: &[u8]) -> Option<Vec<u8>> {
        // Not a fragment: pass through.
        if payload.first() != Some(&engine_webrtc_protocol::channel::OP_DELTA) {
            return Some(payload.to_vec());
        }
        let (header, chunk) = DeltaHeader::parse(payload)?;
        if header.is_single() {
            return Some(chunk.to_vec());
        }
        if header.frag_idx == 0 {
            self.buf.clear();
            self.buf.reserve(MAX_DELTA_BODY.min(1 << 20));
        }
        self.buf.extend_from_slice(chunk);
        if header.frag_idx + 1 == header.frag_total {
            Some(std::mem::take(&mut self.buf))
        } else {
            None
        }
    }
}

/// Strip the WS delta prefix and the session-id/ack/tail envelope.
/// Mirrors cyg.js `_processDeltaPayload`.
pub fn unwrap_payload(payload: &[u8]) -> Result<RawFrame<'_>, String> {
    let mut at = 0usize;
    // WS prefix: [0x11][0xA5 "delta"] or bare [0xA5 "delta"] — the 0xA5 tag
    // is always present.
    if payload.get(at) == Some(&0x11) {
        at += 1;
    }
    if payload.get(at) != Some(&0xA5) {
        return Err("delta prefix missing".into());
    }
    at += 1;
    const TAG: &[u8] = b"delta";
    if payload.get(at..at + TAG.len()) != Some(TAG) {
        return Err("delta tag mismatch".into());
    }
    at += TAG.len();
    // envelope: [your_id i64][section][acked u16][tick u32][stamp u16]
    if payload.len() < at + 8 + 8 {
        return Err("delta payload too short".into());
    }
    let body = &payload[at + 8..payload.len() - 8];
    // envelope tail: [acked_seq u16][server_tick u32][client_stamp u16]
    let tail = &payload[payload.len() - 8..];
    let acked_seq = u16::from_le_bytes([tail[0], tail[1]]);
    let server_tick = u32::from_le_bytes([tail[2], tail[3], tail[4], tail[5]]);
    Ok(RawFrame {
        section: body,
        acked_seq,
        server_tick,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unwraps_bare_and_prefixed_envelopes() {
        let mut payload = vec![0xA5u8];
        payload.extend_from_slice(b"delta");
        payload.extend_from_slice(&123i64.to_le_bytes());
        payload.extend_from_slice(b"SECTION");
        payload.extend_from_slice(&5u16.to_le_bytes());
        payload.extend_from_slice(&777u32.to_le_bytes());
        payload.extend_from_slice(&9u16.to_le_bytes());
        let bare = unwrap_payload(&payload).unwrap();
        assert_eq!(bare.section, b"SECTION");
        assert_eq!(bare.acked_seq, 5);
        assert_eq!(bare.server_tick, 777);

        let body = &payload[6..]; // strip the tag — prefixed frames embed the body
        let mut prefixed = vec![0x11u8, 0xA5];
        prefixed.extend_from_slice(b"delta");
        prefixed.extend_from_slice(body);
        // and a bare (no 0x11) variant
        let mut bare_prefixed = vec![0xA5u8];
        bare_prefixed.extend_from_slice(b"delta");
        bare_prefixed.extend_from_slice(&123i64.to_le_bytes());
        bare_prefixed.extend_from_slice(b"SECTION");
        bare_prefixed.extend_from_slice(&5u16.to_le_bytes());
        bare_prefixed.extend_from_slice(&777u32.to_le_bytes());
        bare_prefixed.extend_from_slice(&9u16.to_le_bytes());
        assert_eq!(unwrap_payload(&bare_prefixed).unwrap().section, b"SECTION");
        let frame = unwrap_payload(&prefixed).unwrap();
        assert_eq!(frame.section, b"SECTION");
        assert_eq!(frame.acked_seq, 5);
        assert_eq!(frame.server_tick, 777);
    }

    #[test]
    fn reassembles_fragments_and_passes_whole_frames() {
        let mut r = DeltaReassembler::new();
        let whole = vec![0x99u8]; // not a delta op — passes through
        assert_eq!(r.push(&whole).unwrap(), whole);

        let chunk_a = vec![1u8; 10];
        let chunk_b = vec![2u8; 5];
        let mut f0 = Vec::new();
        f0.push(0x04);
        f0.extend_from_slice(&7u16.to_le_bytes());
        f0.extend_from_slice(&999u32.to_le_bytes());
        f0.push(0);
        f0.push(2);
        f0.extend_from_slice(&chunk_a);
        // frag_idx 1 of 2
        let mut f1 = Vec::new();
        f1.push(0x04);
        f1.extend_from_slice(&7u16.to_le_bytes());
        f1.extend_from_slice(&999u32.to_le_bytes());
        f1.push(1);
        f1.push(2);
        f1.extend_from_slice(&chunk_b);

        assert!(r.push(&f0).is_none());
        let assembled = r.push(&f1).unwrap();
        let mut want = chunk_a;
        want.extend_from_slice(&chunk_b);
        assert_eq!(assembled, want);
    }
}
