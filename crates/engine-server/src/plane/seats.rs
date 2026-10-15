//! Seat tokens, owned by Agent 3C.
//!
//! NOTE (A6 / integration): this file is a verbatim move and needs the
//! workspace deps `hmac = "0.12"` + `sha2 = "0.10"`, which are NOT yet in
//! the (frozen) engine-server Cargo.toml. Add them at integration or the
//! two `use` lines below will not resolve.
//!
//! Seat tokens: `b64url(payload) + "." + b64url(hmac_sha256(secret, payload))`
//! with `payload = "<room>:<exp_unix_secs>"` (exact bytes, ASCII). Verification
//! is constant-time MAC compare + expiry. `mint` exists for tests/tooling until
//! the Phase 1 matchmake API issues real seats.

use std::time::{SystemTime, UNIX_EPOCH};

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Debug)]
pub struct SeatGrant {
    pub room: String,
}

pub struct SeatRegistry {
    secret: Vec<u8>,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

const B64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// RFC4648 URL-safe base64, no padding. Tiny table-driven encoder: the payload
/// is a short ASCII string and the MAC is 32 bytes, so throughput never matters.
fn b64url_encode(input: &[u8]) -> String {
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

/// Strict unpadded b64url: any byte outside the alphabet, a `=`, or a
/// `len % 4 == 1` remainder rejects the whole token.
fn b64url_decode(input: &str) -> Option<Vec<u8>> {
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

impl SeatRegistry {
    pub fn new(secret: &str) -> SeatRegistry {
        SeatRegistry { secret: secret.as_bytes().to_vec() }
    }

    pub fn mint(&self, room: &str, ttl_secs: u64) -> String {
        let payload = format!("{room}:{}", now_unix().saturating_add(ttl_secs));
        let mut mac = HmacSha256::new_from_slice(&self.secret)
            .expect("HMAC accepts keys of any length");
        mac.update(payload.as_bytes());
        let tag = mac.finalize().into_bytes();
        format!("{}.{}", b64url_encode(payload.as_bytes()), b64url_encode(&tag))
    }

    pub fn verify(&self, token: &str) -> Option<SeatGrant> {
        let (payload_b64, sig_b64) = token.split_once('.')?;
        // Payload never contains '.', so a second dot means a forged layout.
        if payload_b64.is_empty() || sig_b64.is_empty() || sig_b64.contains('.') {
            return None;
        }
        let payload = b64url_decode(payload_b64)?;
        let sig = b64url_decode(sig_b64)?;

        let mut mac = HmacSha256::new_from_slice(&self.secret).ok()?;
        mac.update(&payload);
        // verify_slice is constant-time (and length-checked); never swap in ==.
        mac.verify_slice(&sig).ok()?;

        let text = std::str::from_utf8(&payload).ok()?;
        let (room, exp) = text.rsplit_once(':')?;
        let exp: u64 = exp.parse().ok()?;
        if room.is_empty() || room.contains('.') || exp <= now_unix() {
            return None;
        }
        Some(SeatGrant { room: room.to_string() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg() -> SeatRegistry {
        SeatRegistry::new("test-secret")
    }

    #[test]
    fn mint_verify_roundtrip() {
        let token = reg().mint("tank-3d", 300);
        assert!(token.contains('.'), "token shape is payload.sig: {token}");
        let grant = reg().verify(&token).expect("valid token must verify");
        assert_eq!(grant.room, "tank-3d");
    }

    #[test]
    fn b64url_codec_roundtrip() {
        for sample in [
            b"a".as_slice(),
            b"ab",
            b"abc",
            b"abcd",
            b"tank-3d:1700000000",
            b"\xff\xfe\x01\x80",
        ] {
            let enc = b64url_encode(sample);
            assert!(!enc.contains('='));
            assert_eq!(b64url_decode(&enc).as_deref(), Some(sample));
        }
    }

    #[test]
    fn expired_rejected() {
        // ttl 0 ⇒ exp == now, and the contract is exp must be strictly > now.
        let token = reg().mint("tank-3d", 0);
        assert!(reg().verify(&token).is_none());
    }

    #[test]
    fn tampered_mac_rejected() {
        let token = reg().mint("tank-3d", 300);
        let (payload, sig) = token.split_once('.').unwrap();
        let mut bytes: Vec<char> = sig.chars().collect();
        bytes[0] = if bytes[0] == 'A' { 'B' } else { 'A' };
        let tampered = format!("{payload}.{}", bytes.iter().collect::<String>());
        assert!(reg().verify(&tampered).is_none());

        // Swap the MAC onto a different payload: also rejected.
        let other = reg().mint("other-room", 300);
        let (_, other_sig) = other.split_once('.').unwrap();
        assert!(reg().verify(&format!("{payload}.{other_sig}")).is_none());
    }

    #[test]
    fn wrong_secret_rejected() {
        let token = reg().mint("tank-3d", 300);
        assert!(SeatRegistry::new("other-secret").verify(&token).is_none());
    }

    #[test]
    fn garbage_rejected_no_panic() {
        for bad in [
            "", ".", "..", "no-dot", "a.b", "A.B", "@@@.###", "AA==.AA==",
            "QQ.a", // len % 4 == 1 half
            "QQQ.aaa",
            &format!("{}.AAAAAAAA", b64url_encode(b"tank-3d:notanumber")),
            &format!("{}.AAAAAAAA", b64url_encode(b":19999999999")),
        ] {
            assert!(reg().verify(bad).is_none(), "must reject: {bad:?}");
        }
    }
}
