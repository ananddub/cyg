use hmac::Mac;

use super::b64::{b64url, b64url_decode};
use super::registry::{HmacSha256, SeatRegistry};
use super::token::Seat;

impl SeatRegistry {
    /// Constant-time MAC check against one secret.
    fn mac_matches(&self, secret: &[u8], payload: &[u8], presented: &[u8]) -> bool {
        let Ok(mut mac) = <HmacSha256 as Mac>::new_from_slice(secret) else {
            return false;
        };
        mac.update(payload);
        mac.verify_slice(presented).is_ok()
    }

    pub(super) fn sign(&self, room: &str, expires_at: u64) -> String {
        let payload = format!("{room}:{expires_at}");
        let payload = payload.as_bytes();
        let mut mac =
            <HmacSha256 as Mac>::new_from_slice(&self.secret).expect("hmac accepts any key length");
        mac.update(payload);
        format!(
            "{}.{}",
            b64url(payload),
            b64url(&mac.finalize().into_bytes())
        )
    }

    pub fn verify(&mut self, token: &str) -> Option<Seat> {
        match self.check(token) {
            Some(seat) => Some(seat),
            None => {
                self.rejected += 1;
                None
            }
        }
    }

    pub fn is_valid(&self, token: &str) -> bool {
        self.check(token).is_some()
    }

    fn check(&self, token: &str) -> Option<Seat> {
        let (payload_b64, mac_b64) = token.split_once('.')?;
        if payload_b64.is_empty() || mac_b64.is_empty() || mac_b64.contains('.') {
            return None;
        }
        let payload = b64url_decode(payload_b64)?;
        let presented = b64url_decode(mac_b64)?;
        if !self.mac_matches(&self.secret, &payload, &presented)
            && (self.previous.is_empty() || !self.mac_matches(&self.previous, &payload, &presented))
        {
            return None;
        }
        let text = std::str::from_utf8(&payload).ok()?;
        let (room, exp) = text.rsplit_once(':')?;
        let exp: u64 = exp.parse().ok()?;
        if room.is_empty() || room.contains('.') {
            return None;
        }
        if exp <= Self::now_secs() {
            return None;
        }
        Some(self.by_token.get(token).cloned().unwrap_or(Seat {
            id: format!("ext-{room}-{exp}"),
            room: room.to_string(),
            token: token.to_string(),
            expires_at: exp,
        }))
    }

    pub fn release(&mut self, token: &str) -> Option<Seat> {
        self.by_token.remove(token)
    }

    pub fn retire_expired(&mut self) {
        let now = Self::now_secs();
        self.by_token.retain(|_, seat| seat.expires_at > now);
        self.next_expiry.retain(|_, expiry| *expiry > now);
    }

    pub fn outstanding(&self) -> usize {
        self.by_token.len()
    }

    pub fn counts(&self) -> (u64, u64) {
        (self.minted, self.rejected)
    }

    pub fn refused(&self) -> u64 {
        self.refused
    }

    pub fn seat_bound(&self) -> usize {
        self.max_outstanding
    }
}
