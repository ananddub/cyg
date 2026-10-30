//! Signs the body the control plane puts on the wire, and says so in words the
//! receiver can check itself against.
//!
//! A delivery carries the body byte for byte as it was signed; the signature is
//! `v1=<hex of HMAC-SHA256(secret, "<timestamp>.<body>"))`, so a receiver
//! recomputes over `X-CYG-Timestamp + "." + raw body` and compares in constant
//! time. A timestamp further from the present than [`REPLAY_WINDOW`] is refused
//! whatever its signature, because a captured delivery stays valid forever
//! otherwise.

use hmac::{Hmac, Mac};
use sha2::Sha256;

/// A captured delivery is accepted this long after its timestamp, and no longer.
pub const REPLAY_WINDOW: i64 = 300;
pub const SIGNATURE_HEADER: &str = "X-CYG-Signature";
pub const TIMESTAMP_HEADER: &str = "X-CYG-Timestamp";
pub const DELIVERY_HEADER: &str = "X-CYG-Delivery";
const SCHEME: &str = "v1";

type HmacSha256 = Hmac<Sha256>;

/// The string a receiver has to reproduce exactly: the timestamp, a dot, then
/// the raw body.
pub fn signed_string(timestamp: &str, body: &str) -> String {
    format!("{timestamp}.{body}")
}

pub fn signature(secret: &str, timestamp: &str, body: &str) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts a key of any length");
    mac.update(signed_string(timestamp, body).as_bytes());
    format!("{SCHEME}={}", hex::encode(mac.finalize().into_bytes()))
}

fn constant_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

/// True when `presented` is the signature `secret` produces for this exact
/// timestamp and body, and the timestamp is inside the replay window.
pub fn verifies(secret: &str, timestamp: &str, body: &str, presented: &str, now: i64) -> bool {
    let Ok(at) = timestamp.parse::<i64>() else {
        return false;
    };
    if now - at > REPLAY_WINDOW || at - now > REPLAY_WINDOW {
        return false;
    }
    let Some(claimed) = presented.strip_prefix(&format!("{SCHEME}=")) else {
        return false;
    };
    let Ok(claimed) = hex::decode(claimed) else {
        return false;
    };
    let expected = signature(secret, timestamp, body);
    let expected = expected.strip_prefix(&format!("{SCHEME}=")).unwrap_or("");
    let Ok(expected) = hex::decode(expected) else {
        return false;
    };
    constant_eq(&claimed, &expected)
}

/// What the console shows an operator, so the receiver can be written from it
/// rather than guessed at.
pub fn spec() -> serde_json::Value {
    serde_json::json!({
        "algorithm": "HMAC-SHA256",
        "signed_string": "<X-CYG-Timestamp>.<raw request body>",
        "signature_header": SIGNATURE_HEADER,
        "signature_format": "v1=<lowercase hex>",
        "timestamp_header": TIMESTAMP_HEADER,
        "delivery_header": DELIVERY_HEADER,
        "replay_window_seconds": REPLAY_WINDOW,
        "content_type": "application/json",
        "note": "verify the body bytes exactly as received, before parsing them",
    })
}

#[cfg(test)]
mod tests;
