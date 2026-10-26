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

    // CORS: the SDK sends requests with credentials mode 'include', so the wildcard origin is rejected — echo the request's exact Origin instead.
    let cors_allow = format!("Access-Control-Allow-Origin: {}\r\n", req.origin);

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
