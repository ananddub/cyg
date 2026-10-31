use hmac::{Hmac, Mac};
use sha1::Sha1;
use sha2::{Digest, Sha256};

const DIGITS: u32 = 6;
const STEP_SECS: u64 = 30;
pub const DRIFT_STEPS: u64 = 1;
const SECRET_BYTES: usize = 20;

pub fn generate_secret() -> String {
    let mut bytes = [0u8; SECRET_BYTES];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
    data_encoding::BASE32_NOPAD.encode(&bytes)
}

pub fn code_at(secret: &str, unix_time: i64) -> Option<String> {
    let counter = step_counter(unix_time);
    let code = hotp(secret, counter, DIGITS)?;
    Some(format!("{code:0width$}", width = DIGITS as usize))
}

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
    (-window..=window).any(|delta| {
        let candidate = counter + delta;
        candidate >= 0 && hotp(secret, candidate as u64, DIGITS) == Some(entered)
    })
}

pub fn otpauth_uri(secret: &str, issuer: &str, account: &str) -> String {
    let label = format!("{}:{}", encode_component(issuer), encode_component(account));
    let query = format!(
        "secret={secret}&issuer={}&algorithm=SHA1&digits={DIGITS}&period={STEP_SECS}",
        encode_component(issuer)
    );
    format!("otpauth://totp/{label}?{query}")
}

pub fn generate_recovery_codes(count: usize) -> Vec<String> {
    (0..count)
        .map(|_| {
            let mut bytes = [0u8; 8];
            rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
            let raw = data_encoding::BASE32_NOPAD.encode(&bytes);
            raw[..4].to_owned() + "-" + &raw[4..8] + "-" + &raw[8..]
        })
        .collect()
}

pub fn hash_recovery_code(code: &str) -> String {
    let digest = Sha256::digest(code.trim().to_ascii_uppercase().as_bytes());
    hex::encode(digest)
}

fn step_counter(unix_time: i64) -> u64 {
    (unix_time.max(0) as u64) / STEP_SECS
}

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
