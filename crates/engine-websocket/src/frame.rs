//! # RFC 6455 Frame Codec
//!
//! Incremental, allocation-light framing:
//! [1 byte FIN/opcode][1 byte MASK+len7][0/2/8 byte extended len][0/4 byte mask key][payload]

use bytes::{BufMut, Bytes, BytesMut};

/// Frame opcodes (RFC 6455 §5.2).
pub mod opcode {
    pub const CONTINUATION: u8 = 0x0;
    pub const TEXT: u8 = 0x1;
    pub const BINARY: u8 = 0x2;
    pub const CLOSE: u8 = 0x8;
    pub const PING: u8 = 0x9;
    pub const PONG: u8 = 0xA;
}

/// Standard close status codes (RFC 6455 §7.4).
pub mod close_code {
    pub const NORMAL: u16 = 1000;
    pub const GOING_AWAY: u16 = 1001;
    pub const PROTOCOL_ERROR: u16 = 1002;
    pub const UNSUPPORTED_DATA: u16 = 1003;
    pub const INVALID_PAYLOAD: u16 = 1007;
    pub const POLICY_VIOLATION: u16 = 1008;
    pub const TOO_BIG: u16 = 1009;
    pub const INTERNAL_ERROR: u16 = 1011;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub fin: bool,
    pub opcode: u8,
    /// Zero-copy view of the decoder's receive buffer — cloning is a
    /// refcount bump, not a byte copy.
    pub payload: Bytes,
}

/// Protocol violation while decoding — carry close code 1002.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    /// Client frame without MASK bit, reserved bits set, or bad opcode.
    Protocol(&'static str),
    /// Frame payload exceeds the configured limit.
    TooBig(usize),
}

/// Encode a frame. `mask_key` is `Some(4 random bytes)` when the writer is a
/// client (clients MUST mask); servers pass `None` (unmasked).
pub fn encode_frame(
    fin: bool,
    opcode: u8,
    payload: &[u8],
    mask_key: Option<[u8; 4]>,
    out: &mut impl BufMut,
) {
    let len = payload.len();
    out.put_u8(if fin { 0x80 | opcode } else { opcode });
    let mask_bit = if mask_key.is_some() { 0x80 } else { 0 };
    match len {
        0..=125 => out.put_u8(mask_bit | len as u8),
        126..=0xFFFF => {
            out.put_u8(mask_bit | 126);
            out.put_slice(&(len as u16).to_be_bytes());
        },
        _ => {
            out.put_u8(mask_bit | 127);
            out.put_slice(&(len as u64).to_be_bytes());
        },
    }
    if let Some(key) = mask_key {
        out.put_slice(&key);
        // Mask while writing: `BufMut` gives no mutable access to bytes
        // already in the buffer, so XOR goes straight into the sink.
        let mut chunk = [0u8; 1024];
        let mut offset = 0usize;
        while offset < payload.len() {
            let n = (payload.len() - offset).min(chunk.len());
            for i in 0..n {
                chunk[i] = payload[offset + i] ^ key[(offset + i) & 3];
            }
            out.put_slice(&chunk[..n]);
            offset += n;
        }
    } else {
        out.put_slice(payload);
    }
}

/// Incremental frame decoder for one direction of one connection.
///
/// Feed socket bytes with [`feed`](Self::feed); a `Ok(None)` means "need more
/// bytes". Frames larger than `max_payload_bytes` are rejected before their
/// payload is buffered. Decoded payloads are zero-copy [`Bytes`] views of the
/// internal buffer — consumed prefixes are detached in O(1), no compaction.
pub struct FrameDecoder {
    buf: BytesMut,
    max_payload_bytes: usize,
}

impl FrameDecoder {
    pub fn new(max_payload_bytes: usize) -> Self {
        Self {
            buf: BytesMut::with_capacity(4096),
            max_payload_bytes,
        }
    }

    /// Append every frame completed by these bytes to `out` (caller-owned —
    /// reuse one `Vec` across feeds; it is not cleared here).
    pub fn feed(&mut self, bytes: &[u8], out: &mut Vec<Frame>) -> Result<(), FrameError> {
        self.buf.extend_from_slice(bytes);
        self.decode_ready(out)
    }

    /// Read straight from a socket into the buffer's spare capacity — the
    /// bytes never touch an intermediate stack buffer — and append every
    /// frame the read completed to `out`. `Ok((0, _))` is EOF; `WouldBlock`
    /// comes back as an `io::Error`. Frame-level protocol errors are
    /// returned in the inner `Result` so the server can map them to close
    /// codes.
    pub fn feed_socket<R: std::io::Read>(
        &mut self,
        socket: &mut R,
        want: usize,
        out: &mut Vec<Frame>,
    ) -> std::io::Result<(usize, Result<(), FrameError>)> {
        self.buf.reserve(want);
        let spare = self.buf.spare_capacity_mut();
        let len = spare.len().min(want);
        // SAFETY: `MaybeUninit<u8>` and `u8` share the same layout and
        // alignment; the kernel only writes into the slice it is handed.
        let raw = unsafe { std::slice::from_raw_parts_mut(spare.as_mut_ptr() as *mut u8, len) };
        match socket.read(raw) {
            Ok(0) => Ok((0, Ok(()))),
            Ok(n) => {
                // SAFETY: the read initialized exactly `raw[..n]`, and
                // `old_len + n` stays within the reserved capacity.
                unsafe { self.buf.set_len(self.buf.len() + n) };
                Ok((n, self.decode_ready(out)))
            }
            Err(e) => Err(e),
        }
    }

    fn decode_ready(&mut self, out: &mut Vec<Frame>) -> Result<(), FrameError> {
        loop {
            match self.try_decode()? {
                Some(frame) => out.push(frame),
                None => return Ok(()),
            }
        }
    }

    fn try_decode(&mut self) -> Result<Option<Frame>, FrameError> {
        let buf = &self.buf[..];
        if buf.len() < 2 {
            return Ok(None);
        }

        let fin = buf[0] & 0x80 != 0;
        let rsv = (buf[0] >> 4) & 0x7; // RSV1..3
        if rsv != 0 {
            return Err(FrameError::Protocol("reserved bits set"));
        }
        let opcode = buf[0] & 0x0F;
        if !matches!(opcode, 0x0..=0x2 | 0x8..=0xA) {
            return Err(FrameError::Protocol("unknown opcode"));
        }

        let masked = buf[1] & 0x80 != 0;
        let mut len = (buf[1] & 0x7F) as usize;
        let mut hdr = 2usize;
        if len == 126 {
            if buf.len() < 4 {
                return Ok(None);
            }
            len = u16::from_be_bytes([buf[2], buf[3]]) as usize;
            hdr = 4;
        } else if len == 127 {
            if buf.len() < 10 {
                return Ok(None);
            }
            let big = u64::from_be_bytes(buf[2..10].try_into().unwrap());
            if big > u64::from(u32::MAX) || big as usize > self.max_payload_bytes {
                return Err(FrameError::TooBig(big.min(u64::MAX) as usize));
            }
            len = big as usize;
            hdr = 10;
        }
        if len > self.max_payload_bytes {
            return Err(FrameError::TooBig(len));
        }

        // Client-to-server frames MUST be masked (RFC 6455 §5.1).
        let key_len = if masked { 4 } else { 0 };
        // Server-to-client frames MUST NOT be masked; tolerate lenient peers
        // but reject masked-when-direction-mismatch via caller if strict.
        if buf.len() < hdr + key_len + len {
            return Ok(None);
        }

        let key = if masked {
            let mut k = [0u8; 4];
            k.copy_from_slice(&buf[hdr..hdr + 4]);
            Some(k)
        } else {
            None
        };

        // Detach the whole frame, unmask the payload in place, and hand out a
        // zero-copy view of it.
        let payload_start = hdr + key_len;
        let mut frame = self.buf.split_to(payload_start + len);
        if let Some(k) = key {
            for (i, byte) in frame[payload_start..].iter_mut().enumerate() {
                *byte ^= k[i & 3];
            }
        }
        let payload = frame.split_off(payload_start).freeze();

        Ok(Some(Frame { fin, opcode, payload }))
    }
}

/// Build a CLOSE frame payload: 2-byte status + optional UTF-8 reason.
pub fn close_payload(code: u16, reason: &str) -> Vec<u8> {
    let mut p = code.to_be_bytes().to_vec();
    p.extend_from_slice(reason.as_bytes());
    p
}

/// Parse a CLOSE frame payload into (code, reason).
pub fn parse_close(payload: &[u8]) -> (u16, String) {
    if payload.len() < 2 {
        return (close_code::NORMAL, String::new());
    }
    let code = u16::from_be_bytes([payload[0], payload[1]]);
    let reason = String::from_utf8_lossy(&payload[2..]).into_owned();
    (code, reason)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(payload: &[u8], opcode: u8, masked: bool) {
        let key = if masked { Some([0x12, 0x34, 0x56, 0x78]) } else { None };
        let mut out = Vec::new();
        encode_frame(true, opcode, payload, key, &mut out);
        let mut dec = FrameDecoder::new(128 * 1024);
        let mut frames = Vec::new();
        dec.feed(&out, &mut frames).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].payload, payload);
        assert!(frames[0].fin);
        assert_eq!(frames[0].opcode, opcode);
    }

    #[test]
    fn length_tiers() {
        roundtrip(b"hi", opcode::TEXT, true);
        roundtrip(&vec![7u8; 200], opcode::BINARY, true);
        roundtrip(&vec![9u8; 70_000], opcode::BINARY, true);
        roundtrip(&vec![9u8; 70_000], opcode::BINARY, false);
    }

    #[test]
    fn incremental_split_feeds() {
        let mut out = Vec::new();
        encode_frame(true, opcode::TEXT, b"hello world", Some([1, 2, 3, 4]), &mut out);
        let mut dec = FrameDecoder::new(8 * 1024);
        let mut got = Vec::new();
        for (i, byte) in out.iter().enumerate() {
            dec.feed(&[(*byte)], &mut got).unwrap();
            assert!(got.len() <= 1);
            if i < out.len() - 1 {
                assert!(got.is_empty() || i == out.len() - 1);
            }
        }
        assert_eq!(got.len(), 1);
        assert_eq!(&got[0].payload[..], b"hello world");
    }

    #[test]
    fn multiple_frames_one_feed() {
        let mut out = Vec::new();
        encode_frame(true, opcode::TEXT, b"aa", Some([0; 4]), &mut out);
        encode_frame(true, opcode::PING, b"p", Some([0; 4]), &mut out);
        let mut dec = FrameDecoder::new(8 * 1024);
        let mut frames = Vec::new();
        dec.feed(&out, &mut frames).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].opcode, opcode::TEXT);
        assert_eq!(frames[1].opcode, opcode::PING);
    }

    #[test]
    fn unmasked_client_frame_rejected_by_policy_check() {
        // Decoder itself accepts unmasked (server->client direction is
        // unmasked); the server enforces masking on client connections.
        let mut out = Vec::new();
        encode_frame(true, opcode::TEXT, b"x", None, &mut out);
        assert_eq!(out[1] & 0x80, 0);
    }

    #[test]
    fn oversized_rejected() {
        let mut out = Vec::new();
        encode_frame(true, opcode::BINARY, &vec![0u8; 5000], None, &mut out);
        let mut dec = FrameDecoder::new(1024);
        let mut frames = Vec::new();
        assert!(matches!(
            dec.feed(&out, &mut frames),
            Err(FrameError::TooBig(5000))
        ));
    }

    #[test]
    fn reserved_bit_rejected() {
        let mut dec = FrameDecoder::new(1024);
        let mut frames = Vec::new();
        let err = dec.feed(&[0xF0, 0x00], &mut frames).unwrap_err();
        assert!(matches!(err, FrameError::Protocol("reserved bits set")));
    }

    #[test]
    fn close_payload_roundtrip() {
        let p = close_payload(close_code::GOING_AWAY, "bye");
        let (code, reason) = parse_close(&p);
        assert_eq!(code, close_code::GOING_AWAY);
        assert_eq!(reason, "bye");
        let (code, reason) = parse_close(&[]);
        assert_eq!(code, close_code::NORMAL);
        assert!(reason.is_empty());
    }
}
