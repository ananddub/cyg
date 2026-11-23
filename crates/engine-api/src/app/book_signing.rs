//! Signed server requests (`API Design Patterns`, ch. 30).
//!
//! A server-to-server call that authenticates with a Basic server key must also
//! prove it knew that key when it built *this* request. The signature covers
//! the method, the path and query, a SHA-256 of the body, a timestamp and a
//! nonce:
//!
//! ```text
//! v1 = HMAC-SHA256(secret, METHOD \n PATH?QUERY \n SHA256(body) \n TIMESTAMP \n NONCE)
//! ```
//!
//! Headers are `X-Cyg-Timestamp`, `X-Cyg-Nonce`, `X-Cyg-Signature: v1=<hex>`.
//! A request outside the five-minute window, a replayed nonce, or a digest that
//! does not match is refused before routing.
//!
//! `CYG_SIGNING=off` turns this off for local work; `optional` verifies a
//! signature when one is present. The default is `required` for Basic requests.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, Request, StatusCode, header};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use crate::error::V2Error;

const TIMESTAMP: &str = "x-cyg-timestamp";
const NONCE: &str = "x-cyg-nonce";
const SIGNATURE: &str = "x-cyg-signature";
const SKEW: i64 = 300;
const MAX_BODY: usize = 8 * 1024 * 1024;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Required,
    Optional,
    Off,
}

fn mode() -> Mode {
    match std::env::var("CYG_SIGNING").as_deref() {
        Ok("off" | "0" | "false" | "no") => Mode::Off,
        Ok("optional") => Mode::Optional,
        _ => Mode::Required,
    }
}

pub async fn verify(request: Request<Body>) -> Result<Request<Body>, Response<Body>> {
    if mode() == Mode::Off {
        return Ok(request);
    }
    let Some(secret) = basic_secret(request.headers()) else {
        // Bearer sessions are already signed JWTs; only server-key calls carry
        // a shared secret that needs request-level proof.
        return Ok(request);
    };
    let has_signature = request.headers().contains_key(SIGNATURE);
    if !has_signature && mode() == Mode::Optional {
        return Ok(request);
    }
    let (parts, body) = request.into_parts();
    let bytes = axum::body::to_bytes(body, MAX_BODY)
        .await
        .map_err(|_| V2Error::invalid_argument("Signed request body too large.").into_response())?;
    if let Err(error) = check(
        &parts.method.to_string(),
        &parts.uri.to_string(),
        &bytes,
        &parts.headers,
        &secret,
        unix_now(),
    ) {
        return Err(error.into_response());
    }
    Ok(Request::from_parts(parts, Body::from(bytes)))
}

fn basic_secret(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let encoded = value.strip_prefix("Basic ")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let text = String::from_utf8(decoded).ok()?;
    let (user, password) = text.split_once(':')?;
    if password.is_empty() {
        Some(user.to_owned())
    } else {
        Some(password.to_owned())
    }
}

fn check(
    method: &str,
    uri: &str,
    body: &[u8],
    headers: &HeaderMap,
    secret: &str,
    now: i64,
) -> Result<(), V2Error> {
    let unauthorized = |message: &str| V2Error::unauthenticated(message.to_owned());
    let timestamp = headers
        .get(TIMESTAMP)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or_else(|| unauthorized("Signed request timestamp required."))?;
    if (now - timestamp).abs() > SKEW {
        return Err(unauthorized(
            "Signed request timestamp is outside the five-minute window.",
        ));
    }
    let nonce = headers
        .get(NONCE)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .ok_or_else(|| unauthorized("Signed request nonce required."))?;
    let presented = headers
        .get(SIGNATURE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("v1="))
        .ok_or_else(|| unauthorized("Signed request signature required."))?;
    let expected = signature(secret, method, uri, body, timestamp, nonce);
    let expected = hex::decode(expected).expect("HMAC output is hex");
    let presented =
        hex::decode(presented).map_err(|_| unauthorized("Signed request signature is not hex."))?;
    if expected != presented {
        return Err(unauthorized("Signed request signature did not match."));
    }
    remember_nonce(nonce, now)
}

fn remember_nonce(nonce: &str, now: i64) -> Result<(), V2Error> {
    static SEEN: OnceLock<Mutex<BTreeMap<String, i64>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut seen = seen.lock().expect("the nonce cache is never poisoned");
    seen.retain(|_, at| now - *at <= SKEW * 2);
    if seen.insert(nonce.to_owned(), now).is_some() {
        return Err(V2Error::unauthenticated(
            "Signed request nonce has already been used.",
        ));
    }
    Ok(())
}

/// The canonical digest a client and server both compute.
pub fn signature(
    secret: &str,
    method: &str,
    uri: &str,
    body: &[u8],
    timestamp: i64,
    nonce: &str,
) -> String {
    let body_hash = hex::encode(Sha256::digest(body));
    let canonical = format!("{method}\n{uri}\n{body_hash}\n{timestamp}\n{nonce}");
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts a key of any length");
    mac.update(canonical.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Build the three headers for a request. Kept here so tests and in-process
/// clients sign exactly what the server verifies.
pub fn headers(
    secret: &str,
    method: &str,
    uri: &str,
    body: &[u8],
    nonce: &str,
) -> [(&'static str, String); 3] {
    let timestamp = unix_now();
    [
        (TIMESTAMP, timestamp.to_string()),
        (NONCE, nonce.to_owned()),
        (
            SIGNATURE,
            format!(
                "v1={}",
                signature(secret, method, uri, body, timestamp, nonce)
            ),
        ),
    ]
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_inputs_produce_the_same_signature() {
        let one = signature("secret", "POST", "/v1/a?b=1", b"{}", 100, "n1");
        let two = signature("secret", "POST", "/v1/a?b=1", b"{}", 100, "n1");
        assert_eq!(one, two);
        assert_ne!(
            one,
            signature("secret", "POST", "/v1/a?b=1", b"{ }", 100, "n1")
        );
        assert_ne!(
            one,
            signature("other", "POST", "/v1/a?b=1", b"{}", 100, "n1")
        );
    }

    #[test]
    fn a_correctly_signed_request_is_accepted() {
        let timestamp = unix_now();
        let nonce = "a-unique-success-nonce";
        let digest = signature("secret", "POST", "/v1/a", b"{}", timestamp, nonce);
        let mut headers = HeaderMap::new();
        headers.insert(
            TIMESTAMP,
            HeaderValue::from_str(&timestamp.to_string()).unwrap(),
        );
        headers.insert(NONCE, HeaderValue::from_static(nonce));
        headers.insert(
            SIGNATURE,
            HeaderValue::from_str(&format!("v1={digest}")).unwrap(),
        );
        check("POST", "/v1/a", b"{}", &headers, "secret", timestamp).unwrap();
    }

    #[test]
    fn a_replayed_nonce_is_refused() {
        let headers = HeaderMap::new();
        let _ = headers;
        let now = unix_now();
        remember_nonce("unique-test-nonce", now).unwrap();
        assert!(remember_nonce("unique-test-nonce", now).is_err());
    }

    #[test]
    fn a_basic_secret_uses_the_password_or_the_key() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Basic c2VydmVyLWtleTo="),
        );
        assert_eq!(basic_secret(&headers).as_deref(), Some("server-key"));
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Basic Z2FtZTpzZWNyZXQ="),
        );
        assert_eq!(basic_secret(&headers).as_deref(), Some("secret"));
    }

    #[test]
    fn the_window_and_status_are_the_books() {
        let mut headers = HeaderMap::new();
        headers.insert(TIMESTAMP, HeaderValue::from_static("1"));
        headers.insert(NONCE, HeaderValue::from_static("n"));
        headers.insert(SIGNATURE, HeaderValue::from_static("v1=00"));
        let error = check("GET", "/v1/a", b"", &headers, "s", 10_000).unwrap_err();
        assert_eq!(error.kind, crate::error::UNAUTHENTICATED);
    }
}
