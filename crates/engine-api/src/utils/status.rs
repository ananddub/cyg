//! Telemetry helpers: the monotonic age of the newest sim sample and the dialable host.

use axum::http::HeaderMap;
use engine_bus::StatusSnapshot;
use engine_bus::clock::mono_ns;

/// `age_ms` against the sim's own monotonic clock.
pub(crate) fn age_ms(snap: &StatusSnapshot) -> u64 {
    let now = mono_ns() / 1_000_000;
    now.saturating_sub(snap.updated_ms)
}

/// The host a client can actually dial. `bind_addr` is what the *server* bound
/// and `0.0.0.0` is not connectable, so echo the authority the caller used,
/// re-pinned to this plane's port, falling back to loopback without a `Host`.
pub(crate) fn client_host(headers: &HeaderMap) -> String {
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(|authority| match authority.rsplit_once(':') {
            // A bare IPv6 host has no port and IPv6 literal ports follow a
            // `]`, so this only strips for IPv4/names.
            Some((h, p))
                if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) && !h.contains(']') =>
            {
                h
            }
            _ => authority,
        })
        .unwrap_or("127.0.0.1");
    match host {
        "0.0.0.0" | "::" | "" | "*" => "127.0.0.1".to_string(),
        other => other.to_string(),
    }
}
