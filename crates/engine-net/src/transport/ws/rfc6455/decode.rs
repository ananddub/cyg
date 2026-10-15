//! Take one frame off the front of the receive buffer.

use bytes::{Bytes, BytesMut};

/// Take one frame off the front of `buf`. `None` when the buffer holds an
/// incomplete frame. Unmasks the payload in place and hands out a refcounted
/// `Bytes` view — no per-frame copy.
pub fn next_ws_frame(buf: &mut BytesMut) -> Option<(u8, bool, Bytes)> {
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
    }

    let mask_len = if is_masked { 4 } else { 0 };
    let total_len = header_len + mask_len + payload_len;

    if buf.len() < total_len {
        return None;
    }

    // Detach the whole frame, then unmask the payload region in place and hand out a zero-copy view of it.
    let mut frame = buf.split_to(total_len);
    let payload_start = header_len + mask_len;
    if is_masked {
        let mask = [
            frame[header_len],
            frame[header_len + 1],
            frame[header_len + 2],
            frame[header_len + 3],
        ];
        for (i, byte) in frame[payload_start..total_len].iter_mut().enumerate() {
            *byte ^= mask[i & 3];
        }
    }
    let payload = frame.split_off(payload_start).freeze();
    Some((ws_opcode, fin, payload))
}
