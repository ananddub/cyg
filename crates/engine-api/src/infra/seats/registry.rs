use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use hmac::Hmac;
use sha2::Sha256;

use super::token::{MAX_OUTSTANDING_SEATS, MAX_TTL_DRIFT_SECS, Seat};

pub(super) type HmacSha256 = Hmac<Sha256>;

/// Bounded registry of minted seats: `mint` sweeps expired seats and then refuses
/// rather than growing, so a flood on the credential-free mint endpoint cannot leak.
pub struct SeatRegistry {
    pub(super) secret: Vec<u8>,
    pub(super) by_token: HashMap<String, Seat>,
    pub(super) next_expiry: HashMap<String, u64>,
    pub(super) minted: u64,
    pub(super) rejected: u64,
    /// Mints refused by the outstanding-seat bound; separate from `rejected`
    /// (an auth failure) so a capacity event is not read as an attack.
    pub(super) refused: u64,
    pub(super) max_outstanding: usize,
}

impl SeatRegistry {
    pub fn new(secret: &str) -> SeatRegistry {
        SeatRegistry::with_seat_bound(secret, MAX_OUTSTANDING_SEATS)
    }

    pub fn with_seat_bound(secret: &str, max_outstanding: usize) -> SeatRegistry {
        SeatRegistry {
            secret: secret.as_bytes().to_vec(),
            by_token: HashMap::new(),
            next_expiry: HashMap::new(),
            minted: 0,
            rejected: 0,
            refused: 0,
            max_outstanding: max_outstanding.max(1),
        }
    }

    pub(super) fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// Returns `None` at the bound (even after sweeping) and for rooms the payload
    /// cannot carry: the verifier splits on the last `:` and the UDP plane refuses a `.`.
    pub fn mint(&mut self, room: &str, ttl_secs: u64) -> Option<Seat> {
        if room.is_empty() || room.contains(':') || room.contains('.') {
            self.rejected += 1;
            return None;
        }
        if self.by_token.len() >= self.max_outstanding {
            self.retire_expired();
            if self.by_token.len() >= self.max_outstanding {
                self.refused += 1;
                return None;
            }
        }
        // The payload carries no nonce, so two seats for the same room in the same
        // second would share a token, a sessionId, and so one player identity on the
        // game plane. Uniqueness comes from the expiry: always strictly above every
        // expiry this room ever used, within the drift cap.
        let base = Self::now_secs().saturating_add(ttl_secs);
        let used = self
            .next_expiry
            .get(room)
            .map(|e| e.saturating_add(1))
            .unwrap_or(0);
        let expires_at = base.max(used);
        if expires_at > base.saturating_add(MAX_TTL_DRIFT_SECS) {
            self.refused += 1;
            return None;
        }
        self.next_expiry.insert(room.to_string(), expires_at);
        let seat = Seat {
            id: format!("{}-{}", Self::now_secs(), self.minted),
            room: room.to_string(),
            token: self.sign(room, expires_at),
            expires_at,
        };
        self.minted += 1;
        self.by_token.insert(seat.token.clone(), seat.clone());
        Some(seat)
    }
}
