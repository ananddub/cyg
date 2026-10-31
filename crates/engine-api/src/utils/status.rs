use axum::http::HeaderMap;
use engine_bus::StatusSnapshot;
use engine_bus::clock::mono_ns;

pub(crate) fn age_ms(snap: &StatusSnapshot) -> u64 {
    let now = mono_ns() / 1_000_000;
    now.saturating_sub(snap.updated_ms)
}

pub(crate) fn client_host(headers: &HeaderMap) -> String {
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(|authority| match authority.rsplit_once(':') {
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
