use std::io::Write;

use super::super::client::WsClient;
use super::super::hooks::ServerCtx;
use super::request::Greeting;
use super::seat::{default_matchmake, parse_json_string};

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

pub(super) fn serve_matchmake(
    client: &mut WsClient,
    req: &Greeting,
    cors_allow: &str,
    ctx: &mut ServerCtx<'_>,
) -> bool {
    let result = match ctx.matchmake_hook {
        Some(hook) => hook(&req.url_method, &req.room_name, &req.url_body),
        None => default_matchmake(
            &req.url_method,
            &req.room_name,
            &req.url_body,
            ctx.retained,
            &client.session_id,
            ctx.game_port,
            ctx.udp_port,
            &ctx.room_variants,
            ctx.clients_meta,
        ),
    };
    let (status, payload) = match result {
        Ok(seat) => {
            if let (Some(sid), Some(token)) = (
                parse_json_string(&seat, "sessionId"),
                parse_json_string(&seat, "reconnectionToken"),
            ) {
                ctx.clients_meta
                    .entry(std::sync::Arc::from(sid.as_str()))
                    .or_default()
                    .seat = Some(token);
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
