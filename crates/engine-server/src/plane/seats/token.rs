use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub struct SeatGrant {
    pub room: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeatError {
    Bad,
    Expired,
    RoomMismatch,
    Replayed,
}

pub(super) fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// One raw-UDP join frame: `[0x20][token_len u8][seat_token]`.
///
/// This is the only parser for the join envelope. Both the host UDP lane and
/// the pinned sim plane call it, so a token cannot mean one thing on one lane
/// and something else on the other.
pub fn join_token(frame: &[u8]) -> Option<&str> {
    if frame.len() < 3 || frame[0] != 0x20 {
        return None;
    }
    let token_len = frame[1] as usize;
    if token_len == 0 || frame.len() != 2 + token_len {
        return None;
    }
    std::str::from_utf8(&frame[2..]).ok()
}

pub(super) const B64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub(super) fn b64url_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64_ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(B64_ALPHABET[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64_ALPHABET[((n >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(B64_ALPHABET[(n & 63) as usize] as char);
        }
    }
    out
}

fn b64url_val(c: u8) -> Option<u32> {
    match c {
        b'A'..=b'Z' => Some((c - b'A') as u32),
        b'a'..=b'z' => Some((c - b'a' + 26) as u32),
        b'0'..=b'9' => Some((c - b'0' + 52) as u32),
        b'-' => Some(62),
        b'_' => Some(63),
        _ => None,
    }
}

pub(super) fn b64url_decode(input: &str) -> Option<Vec<u8>> {
    let bytes = input.as_bytes();
    if bytes.is_empty() || bytes.len() % 4 == 1 || bytes.contains(&b'=') {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let mut bits = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            bits |= b64url_val(c)? << (18 - 6 * i);
        }
        out.push((bits >> 16) as u8);
        if chunk.len() > 2 {
            out.push((bits >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(bits as u8);
        }
    }
    Some(out)
}
