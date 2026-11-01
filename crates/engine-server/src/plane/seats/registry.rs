use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::plane::seats::token::SeatGrant;
use crate::plane::seats::token::b64url_decode;
use crate::plane::seats::token::b64url_encode;
use crate::plane::seats::token::now_unix;

type HmacSha256 = Hmac<Sha256>;

pub struct SeatRegistry {
    secret: Vec<u8>,
    previous: Vec<u8>,
}

impl SeatRegistry {
    pub fn new(secret: &str) -> SeatRegistry {
        SeatRegistry::with_previous(
            secret,
            &engine_config::Rotation::global().previous_seat_secret,
        )
    }

    pub fn with_previous(secret: &str, previous: &str) -> SeatRegistry {
        SeatRegistry {
            secret: secret.as_bytes().to_vec(),
            previous: previous.as_bytes().to_vec(),
        }
    }

    fn mac_ok(&self, secret: &[u8], payload: &[u8], sig: &[u8]) -> bool {
        let Ok(mut mac) = HmacSha256::new_from_slice(secret) else {
            return false;
        };
        mac.update(payload);
        mac.verify_slice(sig).is_ok()
    }

    pub fn mint(&self, room: &str, ttl_secs: u64) -> String {
        let payload = format!("{room}:{}", now_unix().saturating_add(ttl_secs));
        let mut mac =
            HmacSha256::new_from_slice(&self.secret).expect("HMAC accepts keys of any length");
        mac.update(payload.as_bytes());
        let tag = mac.finalize().into_bytes();
        format!(
            "{}.{}",
            b64url_encode(payload.as_bytes()),
            b64url_encode(&tag)
        )
    }

    pub fn verify(&self, token: &str) -> Option<SeatGrant> {
        let (payload_b64, sig_b64) = token.split_once('.')?;
        if payload_b64.is_empty() || sig_b64.is_empty() || sig_b64.contains('.') {
            return None;
        }
        let payload = b64url_decode(payload_b64)?;
        let sig = b64url_decode(sig_b64)?;

        if !self.mac_ok(&self.secret, &payload, &sig)
            && (self.previous.is_empty() || !self.mac_ok(&self.previous, &payload, &sig))
        {
            return None;
        }

        let text = std::str::from_utf8(&payload).ok()?;
        let (room, exp) = text.rsplit_once(':')?;
        let exp: u64 = exp.parse().ok()?;
        if room.is_empty() || room.contains('.') || exp <= now_unix() {
            return None;
        }
        Some(SeatGrant {
            room: room.to_string(),
        })
    }
}
