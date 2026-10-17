//! RFC 6238 time-based one-time passwords: the second factor the console offers.
//!
//! Hotp/TOTP are plain HMAC-SHA1 over a counter (RFC 4226 §5.3, RFC 6238 §4.2), so
//! the whole thing is a few lines of arithmetic over primitives this crate already
//! links — no dependency that only exists for this.

use hmac::{Hmac, Mac};
use sha1::Sha1;
use sha2::{Digest, Sha256};

/// The digits a console client is expected to send. RFC 6238 allows 6 or 8; 6 is
/// what every authenticator app shows, so it is what the console asks for.
const DIGITS: u32 = 6;
/// The step RFC 6238 fixes at 30 seconds. Not configurable: an authenticator app
/// has no way to agree on anything else.
const STEP_SECS: u64 = 30;
/// How many steps either side of "now" a code is accepted for. One step absorbs
/// clock skew without letting a code stay valid for a meaningful window.
pub const DRIFT_STEPS: u64 = 1;
/// 20 bytes is what RFC 4226 §4 requires of the shared secret, and 160 bits is the
/// HMAC-SHA1 block the truncation is designed around.
const SECRET_BYTES: usize = 20;

/// A fresh shared secret, base32 (RFC 4648, no padding) — the encoding every
/// authenticator app and `otpauth://` URI expects.
pub fn generate_secret() -> String {
    let mut bytes = [0u8; SECRET_BYTES];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
    data_encoding::BASE32_NOPAD.encode(&bytes)
}

/// The 6-digit code for one 30-second step. `None` when the stored secret is not
/// decodable base32, which means the row is corrupt rather than that the code is
/// merely wrong — the caller answers both the same way.
pub fn code_at(secret: &str, unix_time: i64) -> Option<String> {
    let counter = step_counter(unix_time);
    let code = hotp(secret, counter, DIGITS)?;
    Some(format!("{code:0width$}", width = DIGITS as usize))
}

/// Whether `code` is the correct code for `unix_time`, or for the step either side
/// of it. Accepting exactly one drift step is the usual server tolerance; a code
/// replayed a minute later is refused.
pub fn verify(secret: &str, code: &str, unix_time: i64) -> bool {
    let code = code.trim();
    if code.len() != DIGITS as usize || !code.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let Ok(entered) = code.parse::<u32>() else {
        return false;
    };
    let counter = step_counter(unix_time) as i64;
    let window = DRIFT_STEPS as i64;
    // A signed window so "one step before the first step" is simply out of range
    // rather than an underflow to guard against.
    (-window..=window).any(|delta| {
        let candidate = counter + delta;
        candidate >= 0 && hotp(secret, candidate as u64, DIGITS) == Some(entered)
    })
}

/// The `otpauth://totp/...` URI an authenticator app scans. The label carries both
/// issuer and account, and the issuer is repeated as a parameter because apps read
/// one or the other.
pub fn otpauth_uri(secret: &str, issuer: &str, account: &str) -> String {
    // The label is `issuer:account` with the colon kept literal — it is the format's
    // separator, not data — so both halves are escaped separately.
    let label = format!("{}:{}", encode_component(issuer), encode_component(account));
    let query = format!(
        "secret={secret}&issuer={}&algorithm=SHA1&digits={DIGITS}&period={STEP_SECS}",
        encode_component(issuer)
    );
    format!("otpauth://totp/{label}?{query}")
}

/// The one-time codes a console user writes down, for a device that is lost. The
/// shape is `hex(sha256(code))`: enough to check a presented code, useless to
/// anyone reading the row.
pub fn generate_recovery_codes(count: usize) -> Vec<String> {
    (0..count)
        .map(|_| {
            let mut bytes = [0u8; 8];
            rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
            // 8 bytes of base32 is 13 characters; grouped for a human to copy.
            let raw = data_encoding::BASE32_NOPAD.encode(&bytes);
            raw[..4].to_owned() + "-" + &raw[4..8] + "-" + &raw[8..]
        })
        .collect()
}

/// The stored form of a recovery code. Compared by hashing the presented value,
/// never by handing back the digits.
pub fn hash_recovery_code(code: &str) -> String {
    let digest = Sha256::digest(code.trim().to_ascii_uppercase().as_bytes());
    hex::encode(digest)
}

/// RFC 6238's counter: whole 30-second steps since the epoch.
fn step_counter(unix_time: i64) -> u64 {
    (unix_time.max(0) as u64) / STEP_SECS
}

/// RFC 4226: HMAC-SHA1 over the big-endian counter, then the dynamic truncation
/// that turns the last 4 bits into an offset into the 20-byte digest.
fn hotp(secret: &str, counter: u64, digits: u32) -> Option<u32> {
    let key = data_encoding::BASE32_NOPAD.decode(secret.as_bytes()).ok()?;
    let mut mac = Hmac::<Sha1>::new_from_slice(&key).expect("hmac accepts any key length");
    mac.update(&counter.to_be_bytes());
    let digest = mac.finalize().into_bytes();
    let offset = (digest[19] & 0x0f) as usize;
    let truncated = u32::from_be_bytes([
        digest[offset],
        digest[offset + 1],
        digest[offset + 2],
        digest[offset + 3],
    ]) & 0x7fff_ffff;
    Some(truncated % 10u32.pow(digits))
}

/// `application/x-www-form-urlencoded` escaping for the label and issuer. The set
/// is the one RFC 3986 leaves unreserved, which is what an authenticator expects.
fn encode_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push_str("%20"),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests;
