//! The unsubscribe link, signed the way a seat is: the payload travels in the
//! clear, base64url, with an HMAC of it appended after a dot.
//!
//! A mail client cannot authenticate into this plane, so the link has to carry
//! its own proof. `user:channel:kind:expiry` is what the recipient is asking
//! for, and the secret is the one only this process has — which makes the
//! difference between a stale link and a forged one a matter of the expiry, not
//! of who holds the URL.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use uuid::Uuid;

use crate::infra::seats::{b64url, b64url_decode};

type HmacSha256 = Hmac<Sha256>;

/// What one link asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsubscribe {
    pub user_id: Uuid,
    pub channel: String,
    /// The kind to switch off, or `*` for the whole channel.
    pub kind: String,
}

fn mac(secret: &str, payload: &[u8]) -> Vec<u8> {
    let mut mac =
        <HmacSha256 as Mac>::new_from_slice(secret.as_bytes()).expect("hmac takes any key length");
    mac.update(payload);
    mac.finalize().into_bytes().to_vec()
}

/// Constant-time, and only ever against a secret that has one: an empty
/// previous secret is no key, and must not verify anything.
fn matches(secret: &str, payload: &[u8], presented: &[u8]) -> bool {
    if secret.is_empty() {
        return false;
    }
    let Ok(mut mac) = <HmacSha256 as Mac>::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(payload);
    mac.verify_slice(presented).is_ok()
}

/// A link that stays useful for `ttl_secs` from now.
pub fn mint(secret: &str, target: &Unsubscribe, ttl_secs: i64) -> String {
    let Unsubscribe {
        user_id,
        channel,
        kind,
    } = target;
    let payload = format!("{user_id}:{channel}:{kind}:{}", unix_now() + ttl_secs);
    let payload = payload.as_bytes();
    format!("{}.{}", b64url(payload), b64url(&mac(secret, payload)))
}

/// The target a link asks for, or nothing when it is forged, malformed or
/// expired. Both secrets are tried so a rotated signing key does not break the
/// links already sitting in people's inboxes.
pub fn verify(secret: &str, previous: &str, token: &str) -> Option<Unsubscribe> {
    let (payload_b64, mac_b64) = token.split_once('.')?;
    if payload_b64.is_empty() || mac_b64.is_empty() || mac_b64.contains('.') {
        return None;
    }
    let payload = b64url_decode(payload_b64)?;
    let presented = b64url_decode(mac_b64)?;
    if !matches(secret, &payload, &presented) && !matches(previous, &payload, &presented) {
        return None;
    }
    let text = std::str::from_utf8(&payload).ok()?;
    let (user, rest) = text.split_once(':')?;
    let (channel, rest) = rest.split_once(':')?;
    let (kind, expiry) = rest.split_once(':')?;
    let Ok(expires) = expiry.parse::<i64>() else {
        return None;
    };
    if expires < unix_now() || channel.is_empty() || kind.is_empty() {
        return None;
    }
    Some(Unsubscribe {
        user_id: Uuid::parse_str(user).ok()?,
        channel: channel.to_owned(),
        kind: kind.to_owned(),
    })
}

fn unix_now() -> i64 {
    chrono::Utc::now().timestamp()
}
