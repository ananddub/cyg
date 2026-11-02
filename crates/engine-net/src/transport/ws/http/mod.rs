
mod matchmake;
mod query;
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
mod tests;
