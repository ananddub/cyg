//! CYG SDK greeting: a WebSocket upgrade or a matchmake HTTP seat
//! reservation.
//!
//! The SDK always POSTs `/matchmake/<method>/<room>` first (plain HTTP) and
//! only opens the WebSocket once it holds the seat-reservation JSON, so both
//! shapes arrive through this one entry point. Each branch is a separate
//! module; `handle_client_greeting` only decides which one runs.

mod matchmake;
mod request;
mod seat;
mod static_get;
mod upgrade;

use super::client::WsClient;
use super::hooks::ServerCtx;
use crate::transport::TransportEvent;
use matchmake::{serve_matchmake, serve_options};
use request::parse_greeting;
use static_get::serve_static;
use upgrade::complete_upgrade;

/// Handle a freshly connected client's first bytes.
/// Returns `true` when the connection must be closed (HTTP path).
pub(super) fn handle_client_greeting(
    client: &mut WsClient,
    peer: &str,
    events_out: &mut Vec<TransportEvent>,
    ctx: &mut ServerCtx<'_>,
) -> bool {
    let req = parse_greeting(&client.recv_buf);

    let cors_allow = cors_origin(&req.origin, &req.host, ctx.allowed_origins)
        .map(|origin| format!("Access-Control-Allow-Origin: {origin}\r\n"))
        .unwrap_or_default();

    if req.is_options {
        return serve_options(client, &cors_allow);
    }

    if req.is_matchmake {
        return serve_matchmake(client, &req, &cors_allow, ctx);
    }

    if req.is_plain_get {
        if let Some(hook) = ctx.http_hook {
            if let Some((content_type, body)) = hook(&req.get_path) {
                return serve_static(client, &content_type, &body, req.is_head);
            }
        }
    }

    complete_upgrade(client, &req, peer, events_out, ctx)
}

/// The SDK fetches with credentials, which forbids `*`, so a granted origin is
/// always echoed exactly. Empty allowlist means same-origin only.
fn cors_origin(origin: &str, host: &str, allowed: &[String]) -> Option<String> {
    if origin.is_empty() || origin == "*" {
        return None;
    }
    if allowed
        .iter()
        .any(|entry| entry == "*" || entry.eq_ignore_ascii_case(origin))
    {
        return Some(origin.to_owned());
    }
    if !allowed.is_empty() {
        return None;
    }
    let authority = origin
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(origin);
    (!host.is_empty() && authority.eq_ignore_ascii_case(host)).then(|| origin.to_owned())
}

#[cfg(test)]
mod tests {
    use super::cors_origin;

    fn allowed(origins: &[&str]) -> Vec<String> {
        origins.iter().map(|o| (*o).to_owned()).collect()
    }

    #[test]
    fn same_origin_is_allowed_by_default() {
        let origin = "https://play.example.com";
        assert_eq!(
            cors_origin(origin, "play.example.com", &[]).as_deref(),
            Some(origin)
        );
        assert_eq!(cors_origin("https://evil.example.com", "play.example.com", &[]), None);
    }

    #[test]
    fn configured_origins_are_exact_and_wildcard_is_explicit() {
        let list = allowed(&["https://studio.example.com"]);
        assert!(cors_origin("https://studio.example.com", "game:7777", &list).is_some());
        assert_eq!(cors_origin("https://other.example.com", "game:7777", &list), None);
        assert!(cors_origin("https://anywhere.example", "game:7777", &allowed(&["*"])).is_some());
    }

    #[test]
    fn a_missing_origin_gets_no_cors_header() {
        assert_eq!(cors_origin("*", "game:7777", &allowed(&["*"])), None);
    }
}
