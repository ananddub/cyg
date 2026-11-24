use bytes::{Buf, Bytes, BytesMut};

pub const MAX_INBOUND_FRAME_SIZE: usize = 1024 * 1024;

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

    buf.advance(payload_start);
    let payload = buf.split_to(payload_len).freeze();
    Some(Ok((ws_opcode, fin, payload)))
}
