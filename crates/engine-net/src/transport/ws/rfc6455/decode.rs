//! Take one frame off the front of the receive buffer.

use bytes::{Buf, Bytes, BytesMut};

/// Hard cap on any single inbound frame. A client declaring more than this in
/// a 64-bit length header is malicious or broken: reject instead of
/// overflowing `usize` (remote panic) or buffering 4 GB (OOM).
pub const MAX_INBOUND_FRAME_SIZE: usize = 1024 * 1024;

/// Take one frame off the front of `buf`. `None` when the buffer holds an
/// incomplete frame. `Err(())` when the peer declared an oversized frame —
/// the connection must be closed (close code 1009). Unmasks the payload in
/// place and hands out a refcounted `Bytes` view — no per-frame copy.
pub fn next_ws_frame(buf: &mut BytesMut) -> Option<Result<(u8, bool, Bytes), ()>> {
    if buf.len() < 2 {
        return None;
    }

    let b0 = buf[0];
    let ws_opcode = b0 & 0x0F;
    let fin = (b0 & 0x80) != 0;
    let b1 = buf[1];
    let is_masked = (b1 & 0x80) != 0;
    let mut payload_len = (b1 & 0x7F) as usize;
    let mut header_len = 2;

    if payload_len == 126 {
        if buf.len() < 4 {
            return None;
        }
        payload_len = u16::from_be_bytes([buf[2], buf[3]]) as usize;
        header_len = 4;
    } else if payload_len == 127 {
        if buf.len() < 10 {
            return None;
        }
        payload_len = u64::from_be_bytes(buf[2..10].try_into().unwrap()) as usize;
        header_len = 10;
        // Reject BEFORE any arithmetic on the attacker-controlled length: the
        // raw u64 cast to usize could otherwise wrap `total_len` into a small
        // number and panic on the slice below (one packet = process death).
        if payload_len > MAX_INBOUND_FRAME_SIZE {
            buf.clear();
            return Some(Err(()));
        }
    }

    let mask_len = if is_masked { 4 } else { 0 };
    let payload_start = header_len + mask_len;
    let total_len = payload_start + payload_len;

    if buf.len() < total_len {
        return None;
    }

    // Unmask the payload in-place using 64-bit words (8 bytes per operation)
    if is_masked {
        let mask = [
            buf[header_len],
            buf[header_len + 1],
            buf[header_len + 2],
            buf[header_len + 3],
        ];
        let mask_u32 = u32::from_ne_bytes(mask);
        let mask_u64 = (mask_u32 as u64) | ((mask_u32 as u64) << 32);

        let slice = &mut buf[payload_start..total_len];
        let mut chunks = slice.chunks_exact_mut(8);
        for chunk in chunks.by_ref() {
            let val = u64::from_ne_bytes(chunk.try_into().unwrap());
            chunk.copy_from_slice(&(val ^ mask_u64).to_ne_bytes());
        }
        let rem = chunks.into_remainder();
        for (i, byte) in rem.iter_mut().enumerate() {
            *byte ^= mask[i & 3];
        }
    }

    // Detach only the payload: advance past the header/mask and split the payload
    buf.advance(payload_start);
    let payload = buf.split_to(payload_len).freeze();
    Some(Ok((ws_opcode, fin, payload)))
}

#[cfg(test)]
mod decode_boundary_tests {
    use super::*;

    fn build_frame(opcode: u8, fin: bool, mask: Option<[u8; 4]>, payload: &[u8]) -> BytesMut {
        let mut buf = BytesMut::new();
        let b0 = (if fin { 0x80 } else { 0 }) | (opcode & 0x0F);
        buf.extend_from_slice(&[b0]);

        let is_masked = mask.is_some();
        let mask_bit = if is_masked { 0x80 } else { 0 };
        let plen = payload.len();

        if plen <= 125 {
            buf.extend_from_slice(&[mask_bit | (plen as u8)]);
        } else if plen <= 65535 {
            buf.extend_from_slice(&[mask_bit | 126]);
            buf.extend_from_slice(&(plen as u16).to_be_bytes());
        } else {
            buf.extend_from_slice(&[mask_bit | 127]);
            buf.extend_from_slice(&(plen as u64).to_be_bytes());
        }

        if let Some(m) = mask {
            buf.extend_from_slice(&m);
            let mut masked_payload = payload.to_vec();
            for (i, b) in masked_payload.iter_mut().enumerate() {
                *b ^= m[i % 4];
            }
            buf.extend_from_slice(&masked_payload);
        } else {
            buf.extend_from_slice(payload);
        }

        buf
    }

    #[test]
    fn test_decode_incomplete_buffers() {
        let mut empty = BytesMut::new();
        assert!(next_ws_frame(&mut empty).is_none());

        let mut one_byte = BytesMut::from(&[0x82][..]);
        assert!(next_ws_frame(&mut one_byte).is_none());

        // Header says 10 bytes payload, but only 5 provided
        let mut partial = build_frame(0x2, true, None, &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        partial.truncate(partial.len() - 3);
        assert!(next_ws_frame(&mut partial).is_none());
    }

    #[test]
    fn test_decode_masked_remainder_chunks() {
        let mask = [0xAA, 0xBB, 0xCC, 0xDD];
        // Test varying lengths to exercise 8-byte chunks + remainder (0 to 15 bytes)
        for len in [0, 1, 3, 7, 8, 9, 15, 16, 17, 33, 124, 125, 200] {
            let original: Vec<u8> = (0..len).map(|i| (i * 7 + 3) as u8).collect();
            let mut buf = build_frame(0x2, true, Some(mask), &original);

            let (op, fin, decoded) = next_ws_frame(&mut buf).expect("frame must decode").expect("not oversize");
            assert_eq!(op, 0x2);
            assert!(fin);
            assert_eq!(&decoded[..], &original[..], "failed for len {}", len);
            assert_eq!(buf.len(), 0, "buffer must be fully consumed");
        }
    }

    #[test]
    fn test_decode_unmasked_frames() {
        let original = vec![42u8; 100];
        let mut buf = build_frame(0x2, true, None, &original);
        let (op, fin, decoded) = next_ws_frame(&mut buf).expect("unmasked frame decode").expect("not oversize");
        assert_eq!(op, 0x2);
        assert!(fin);
        assert_eq!(&decoded[..], &original[..]);
    }
}
