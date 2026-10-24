//! RFC 6238 test vectors and the shape of a generated secret, URI and recovery code.

use super::*;

/// RFC 6238 Appendix B: secret `12345678901234567890` (ASCII), SHA-1. The
/// vectors are 8-digit, so the RFC fixes the HMAC and the truncation, and the
/// 6-digit codes the console asks for are those values shortened.
const RFC_SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

#[test]
fn rfc6238_vectors() {
    let vectors: [(i64, u32); 6] = [
        (59, 94287082_u32),
        (1_111_111_109, 7081804),
        (1_111_111_111, 14050471),
        (1_234_567_890, 89005924),
        (2_000_000_000, 69279037),
        (20_000_000_000, 65353130),
    ];
    for (time, expected) in vectors {
        assert_eq!(
            hotp(RFC_SECRET, (time as u64) / 30, 8),
            Some(expected),
            "RFC 6238 vector at {time}"
        );
    }
}

#[test]
fn the_six_digit_code_is_the_vector_shortened() {
    // 94287082 -> 287082: the last six digits, which is what `% 10^6` gives.
    assert_eq!(code_at(RFC_SECRET, 59).as_deref(), Some("287082"));
}

#[test]
fn a_code_is_accepted_one_step_either_side() {
    let now = 1_700_000_000_i64;
    let current = code_at(RFC_SECRET, now).expect("code");
    let previous = code_at(RFC_SECRET, now - 30).expect("code");
    let stale = code_at(RFC_SECRET, now - 90).expect("code");

    assert!(verify(RFC_SECRET, &current, now));
    assert!(
        verify(RFC_SECRET, &previous, now),
        "one step behind is skew"
    );
    assert!(
        !verify(RFC_SECRET, &stale, now),
        "three steps behind is not"
    );
}

#[test]
fn a_wrong_or_malformed_code_is_refused() {
    let now = 1_700_000_000_i64;
    // A code that is the right shape but the wrong value.
    let wrong = if code_at(RFC_SECRET, now).as_deref() == Some("000000") {
        "111111"
    } else {
        "000000"
    };
    assert!(!verify(RFC_SECRET, wrong, now));
    assert!(!verify(RFC_SECRET, "12345", now), "five digits");
    assert!(!verify(RFC_SECRET, "1234567", now), "seven digits");
    assert!(!verify(RFC_SECRET, "abcdef", now), "not digits");
    assert!(!verify("not-base32!", "123456", now), "corrupt secret");
}

#[test]
fn a_generated_secret_is_base32_and_unique() {
    let a = generate_secret();
    let b = generate_secret();
    assert_ne!(a, b);
    // 20 bytes is exactly 32 base32 characters.
    assert_eq!(a.len(), 32);
    assert!(
        code_at(&a, 1_700_000_000).is_some(),
        "round-trips through base32"
    );
}

#[test]
fn the_provisioning_uri_carries_what_an_app_reads() {
    let uri = otpauth_uri("ABCDEF", "OpenOxide Console", "admin@console.local");
    assert!(uri.starts_with("otpauth://totp/"), "{uri}");
    assert!(uri.contains("secret=ABCDEF"));
    assert!(uri.contains("issuer=OpenOxide%20Console"));
    assert!(uri.contains("digits=6") && uri.contains("period=30"));
    assert!(uri.contains("OpenOxide%20Console:admin%40console.local"));
}

#[test]
fn a_recovery_code_hashes_to_the_stored_form() {
    let codes = generate_recovery_codes(8);
    assert_eq!(codes.len(), 8);
    // 13 base32 characters in a 5-4-4 grouping: 13 + 2 dashes.
    assert!(codes.iter().all(|code| code.len() == 15), "{codes:?}");
    let stored = hash_recovery_code(&codes[0]);
    assert_eq!(stored.len(), 64, "sha256 hex");
    // Case and surrounding space do not change what was written down.
    assert_eq!(hash_recovery_code(&format!(" {} ", codes[0])), stored);
    assert_ne!(hash_recovery_code(&codes[1]), stored);
}
