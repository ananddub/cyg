//! Matchmake over plain HTTP: the CORS preflight and the seat-reservation
//! reply. Both close the connection, because the SDK opens a *separate*
//! WebSocket for the game protocol after it has its seat.

use std::io::Write;

use super::super::client::WsClient;
use super::super::hooks::ServerCtx;
use super::request::Greeting;
use super::seat::{default_matchmake, parse_json_string};

/// Preflight for the SDK's credentialed `fetch()`. Browsers run the game page
/// on a different origin, so these headers are mandatory.
pub(super) fn serve_options(client: &mut WsClient, cors_allow: &str) -> bool {
    let response = format!(
        "HTTP/1.1 204 No Content\r\n\
         {}\
         Access-Control-Allow-Credentials: true\r\n\
         Access-Control-Allow-Methods: POST, OPTIONS\r\n\
         Access-Control-Allow-Headers: Content-Type, Accept\r\n\
         Access-Control-Allow-Private-Network: true\r\n\
         Access-Control-Max-Age: 86400\r\n\
         Connection: close\r\n\r\n",
        cors_allow
    );
    let _ = client.stream.write_all(response.as_bytes());
    client.recv_buf.clear();
    true
}

/// Answer `POST /matchmake/<method>/<room>` with a seat reservation, or a
/// typed rejection the SDK raises as a `MatchMakeError`.
pub(super) fn serve_matchmake(
    client: &mut WsClient,
    req: &Greeting,
    cors_allow: &str,
    ctx: &mut ServerCtx<'_>,
) -> bool {
    // The game hook decides; without one the transport serves the standard Colyseus method whitelist itself.
    let result = match ctx.matchmake_hook {
        Some(hook) => hook(&req.url_method, &req.room_name, &req.url_body),
        None => default_matchmake(
            &req.url_method,
            &req.room_name,
            &req.url_body,
            ctx.retained,
            &client.session_id,
        ),
    };
    let (status, payload) = match result {
        Ok(seat) => {
            // Remember the seat's reconnectionToken so the JOIN_ROOM handshake advertises the same token the SDK reconnects with.
            if let (Some(sid), Some(token)) = (
                parse_json_string(&seat, "sessionId"),
                parse_json_string(&seat, "reconnectionToken"),
            ) {
                ctx.seat_tokens.insert(sid, token);
            }
            ("200 OK", seat)
        }
        Err(err) => (
            "404 Not Found",
            format!(
                "{{\"code\":{},\"error\":\"{}\"}}",
                err.code,
                err.message.replace('"', "'")
            ),
        ),
    };
    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: application/json\r\n{}Access-Control-Allow-Credentials: true\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status,
        cors_allow,
        payload.len(),
        payload
    );
    let _ = client.stream.write_all(response.as_bytes());
    client.recv_buf.clear();
    true
}
