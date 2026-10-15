//! msgpack primitives (LE numerics + strings) used by the join handshake and
//! state blobs.

use bytes::BufMut;

pub(super) fn mp_number(out: &mut impl BufMut, value: f64) {
    if value.is_nan() {
        out.put_u8(0x00);
        return;
    }
    if value.fract() == 0.0 && value.abs() <= 9.007_199_254_740_992e15 {
        let v = value as i64;
        if v >= 0 {
            if v < 0x80 {
                out.put_u8(v as u8);
            } else if v < 0x100 {
                out.put_u8(0xcc);
                out.put_u8(v as u8);
            } else if v < 0x10000 {
                out.put_u8(0xcd);
                out.put_slice(&(v as u16).to_le_bytes());
            } else if v < 0x1_0000_0000 {
                out.put_u8(0xce);
                out.put_slice(&(v as u32).to_le_bytes());
            } else {
                out.put_u8(0xcf);
                out.put_slice(&(v as u64).to_le_bytes());
            }
        } else if v >= -0x20 {
            out.put_u8(0xe0 | ((v + 0x20) as u8));
        } else if v >= -0x80 {
            out.put_u8(0xd0);
            out.put_u8(v as i8 as u8);
        } else if v >= -0x8000 {
            out.put_u8(0xd1);
            out.put_slice(&(v as i16).to_le_bytes());
        } else if v >= -0x8000_0000 {
            out.put_u8(0xd2);
            out.put_slice(&(v as i32).to_le_bytes());
        } else {
            out.put_u8(0xd3);
            out.put_slice(&(v as u64).to_le_bytes());
        }
    } else {
        out.put_u8(0xcb);
        out.put_slice(&value.to_le_bytes());
    }
}

/// msgpack string: fixstr / str8 / str16.
pub(super) fn mp_str(out: &mut impl BufMut, s: &str) {
    let bytes = s.as_bytes();
    let len = bytes.len();
    if len < 0x20 {
        out.put_u8(len as u8 | 0xa0);
    } else if len < 0x100 {
        out.put_u8(0xd9);
        out.put_u8(len as u8);
    } else {
        out.put_u8(0xda);
        out.put_slice(&(len as u16).to_be_bytes());
    }
    out.put_slice(bytes);
}
