//! `ETag` / `If-Match` for the handful of console resources that are edited in
//! place.
//!
//! A versioned row has one honest representation: `ETag: "7"` says "this is
//! generation 7". A client that read generation 7 and wants to write it back
//! sends `If-Match: "7"`; if somebody else already moved the row to 8 the write
//! is refused instead of silently winning. This is what keeps two operators on
//! the same screen from erasing each other's edit.

use axum::http::HeaderMap;

use crate::error::V2Error;

pub const ETAG: &str = "etag";
pub const IF_MATCH: &str = "if-match";

/// The entity tag for one generation of a resource.
pub fn tag(version: i64) -> String {
    format!("\"{version}\"")
}

/// The generation a client expects to be writing over.
///
/// `None` when the request does not say, which keeps the call an unconditional
/// write for clients that predate the header. `*` matches any existing resource;
/// a weak tag (`W/"7"`) is as good as a strong one here, because the comparison
/// is against a version counter rather than a byte-for-byte body.
pub fn expected_version(headers: &HeaderMap) -> Result<Option<i64>, V2Error> {
    let Some(raw) = headers.get(IF_MATCH) else {
        return Ok(None);
    };
    let raw = raw
        .to_str()
        .map_err(|_| V2Error::invalid_argument("If-Match must be ASCII."))?
        .trim();
    if raw == "*" {
        return Ok(None);
    }
    let value = raw
        .strip_prefix("W/")
        .unwrap_or(raw)
        .trim_matches('"')
        .trim();
    value
        .parse::<i64>()
        .map(Some)
        .map_err(|_| V2Error::invalid_argument("If-Match must name a version, e.g. \"7\"."))
}

/// The refusal for a write whose version is stale, naming the version that won
/// so the client can re-read and retry instead of guessing.
pub fn stale(current: i64) -> V2Error {
    V2Error::new(
        crate::error::ABORTED,
        format!(
            "This resource has changed since you read it (now at version {current}). \
             Re-read it and retry."
        ),
    )
}
