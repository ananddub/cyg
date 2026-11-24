use hmac::{Hmac, Mac};
use scc::HashMap;
use sha2::Sha256;

use crate::plane::seats::token::SeatError;
use crate::plane::seats::token::SeatGrant;
use crate::plane::seats::token::b64url_decode;
use crate::plane::seats::token::b64url_encode;
use crate::plane::seats::token::now_unix;

type HmacSha256 = Hmac<Sha256>;

/// Maximum replay entries kept before a forced prune. Prevents unbounded
/// growth if expiry pruning is somehow delayed.
const MAX_REPLAY_ENTRIES: usize = 16_384;

pub struct SeatRegistry {
    secret: Vec<u8>,
    /// token payload (room:exp) -> exp, for replay deny. Entries are pruned
    /// when expired or when the map grows too large.
    seen: HashMap<String, u64>,
}

impl SeatRegistry {
    pub fn new(secret: &str) -> Self {
        Self {
            secret: secret.as_bytes().to_vec(),
            seen: HashMap::new(),
        }
    }

    pub fn mint(&self, room: &str, ttl_secs: u64) -> String {
        let exp = now_unix().saturating_add(ttl_secs);
        let payload = format!("{room}:{exp}");
        let mut mac =
            HmacSha256::new_from_slice(&self.secret).expect("HMAC accepts any key length");
        mac.update(payload.as_bytes());
        let sig = mac.finalize().into_bytes();
        format!("{}.{}", b64url_encode(payload.as_bytes()), b64url_encode(&sig))
    }

    /// Verify a seat token: signature, expiry, optional room match, and
    /// replay deny. A token that passes is consumed — presenting it again
    /// returns `SeatError::Replayed`.
    pub fn verify(&self, token: &str, expected_room: Option<&str>) -> Result<SeatGrant, SeatError> {
        let (payload_b64, sig_b64) = token.rsplit_once('.').ok_or(SeatError::Bad)?;
        let payload = b64url_decode(payload_b64).ok_or(SeatError::Bad)?;
        let sig = b64url_decode(sig_b64).ok_or(SeatError::Bad)?;
        let mut mac = HmacSha256::new_from_slice(&self.secret).map_err(|_| SeatError::Bad)?;
        mac.update(&payload);
        mac.verify_slice(&sig).map_err(|_| SeatError::Bad)?;
        let payload_str = std::str::from_utf8(&payload).map_err(|_| SeatError::Bad)?;
        let (room, exp_str) = payload_str.rsplit_once(':').ok_or(SeatError::Bad)?;
        let exp: u64 = exp_str.parse().map_err(|_| SeatError::Bad)?;
        if room.is_empty() || room.contains('.') {
            return Err(SeatError::Bad);
        }
        let now = now_unix();
        if exp <= now {
            return Err(SeatError::Expired);
        }
        if let Some(expected) = expected_room {
            if room != expected {
                return Err(SeatError::RoomMismatch);
            }
        }

        if self.seen.len() > MAX_REPLAY_ENTRIES || self.seen.len() > 256 {
            self.seen.retain_sync(|_, e| *e > now);
        }
        let key = payload_str.to_string();
        if self.seen.insert_sync(key, exp).is_err() {
            return Err(SeatError::Replayed);
        }

        Ok(SeatGrant {
            room: room.to_string(),
        })
    }

    pub fn verify_frame(
        &self,
        frame: &[u8],
        expected_room: Option<&str>,
    ) -> Result<SeatGrant, SeatError> {
        let token = crate::plane::join_token(frame).ok_or(SeatError::Bad)?;
        self.verify(token, expected_room)
    }

    /// Verify without consuming the token for replay and without room match.
    /// Useful for tests and diagnostics. Does NOT enforce replay deny.
    pub fn verify_unchecked(&self, token: &str) -> Option<SeatGrant> {
        let (payload_b64, sig_b64) = token.rsplit_once('.')?;
        let payload = b64url_decode(payload_b64)?;
        let sig = b64url_decode(sig_b64)?;
        let mut mac = HmacSha256::new_from_slice(&self.secret).ok()?;
        mac.update(&payload);
        mac.verify_slice(&sig).ok()?;
        let payload_str = std::str::from_utf8(&payload).ok()?;
        let (room, exp_str) = payload_str.rsplit_once(':')?;
        let exp: u64 = exp_str.parse().ok()?;
        if room.is_empty() || room.contains('.') || exp <= now_unix() {
            return None;
        }
        Some(SeatGrant {
            room: room.to_string(),
        })
    }
}
