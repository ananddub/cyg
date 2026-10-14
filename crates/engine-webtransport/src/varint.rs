//! RFC 9000 variable-length integer encoding (quic varint).
//!
//! Used everywhere in HTTP/3, QPACK-free header framing, WebTransport
//! stream prefixes, capsules and datagram records.

use bytes::BufMut;

/// Maximum encoded length of a varint.
pub const MAX_VARINT_LEN: usize = 8;

/// Largest value representable (2^62 - 1).
pub const MAX_VALUE: u64 = (1 << 62) - 1;

/// Append `v` in its shortest legal varint encoding.
pub fn encode_into(v: u64, out: &mut impl BufMut) {
    if v <= 0x3f {
        out.put_u8(v as u8);
    } else if v <= 0x3f_ff {
        out.put_slice(&((v as u16) | 0x40_00).to_be_bytes());
    } else if v <= 0x3f_ff_ff_ff {
        out.put_slice(&((v as u32) | 0x8000_0000).to_be_bytes());
    } else {
        out.put_slice(&((v | 0xC000_0000_0000_0000u64) as u64).to_be_bytes());
    }
}

/// Encoded byte length of `v`.
pub fn encoded_len(v: u64) -> usize {
    if v <= 0x3f {
        1
    } else if v <= 0x3f_ff {
        2
    } else if v <= 0x3f_ff_ff_ff {
        4
    } else {
        8
    }
}

/// Convenience wrapper returning a fresh `Vec`.
pub fn encode(v: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(MAX_VARINT_LEN);
    encode_into(v, &mut out);
    out
}

/// Decode one varint from the front of `buf`.
/// Returns `(value, bytes_consumed)` or `None` when more bytes are needed.
pub fn decode(buf: &[u8]) -> Option<(u64, usize)> {
    let &first = buf.first()?;
    let len = 1usize << (first >> 6);
    if buf.len() < len {
        return None;
    }
    let mut v = (first & 0x3f) as u64;
    for &b in &buf[1..len] {
        v = (v << 8) | b as u64;
    }
    Some((v, len))
}

/// Decode a varint that must start exactly at `buf[0]`, consuming it from a
/// cursor. Errs when the buffer is incomplete.
pub fn take(buf: &mut &[u8]) -> Option<u64> {
    let (v, n) = decode(buf)?;
    *buf = &buf[n..];
    Some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_boundaries() {
        for &v in &[0u64, 1, 63, 64, 16383, 16384, 1073741823, 1073741824, MAX_VALUE] {
            let enc = encode(v);
            assert_eq!(enc.len(), encoded_len(v));
            let (dec, used) = decode(&enc).unwrap();
            assert_eq!(dec, v);
            assert_eq!(used, enc.len());
        }
    }

    #[test]
    fn known_vectors() {
        assert_eq!(encode(37), vec![0x25]);
        assert_eq!(encode(15293), vec![0x7b, 0xbd]);
        assert_eq!(encode(494878335), vec![0x9d, 0x7f, 0x3e, 0x7f]);
        assert_eq!(
            encode(151288809941952652),
            vec![0xc2, 0x19, 0x7c, 0x5e, 0xff, 0x14, 0xe8, 0x8c]
        );
    }

    #[test]
    fn incomplete_returns_none() {
        assert!(decode(&[0x80]).is_none()); // needs 4 bytes, has 1
        assert!(decode(&[]).is_none());
    }

    #[test]
    fn take_advances_cursor() {
        let data = [0x25u8, 0x7b, 0xbd];
        let mut cur: &[u8] = &data;
        assert_eq!(take(&mut cur), Some(37));
        assert_eq!(take(&mut cur), Some(15293));
        assert!(cur.is_empty());
    }
}
