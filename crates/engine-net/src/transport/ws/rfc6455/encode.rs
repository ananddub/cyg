//! Encoding server frames: binary messages, control frames and the two ways
//! queued bytes reach the socket.

use std::io::Write;

use bytes::{Buf, BufMut};

use super::super::client::WsClient;

/// Server frames are never masked (RFC 6455).
pub fn encode_ws_frame(payload: &[u8], out: &mut impl BufMut) {
    // Binary frame: FIN bit (0x80) + opcode binary (0x02) = 0x82.
    out.put_u8(0x82);

    let len = payload.len();
    if len <= 125 {
        out.put_u8(len as u8);
    } else if len <= 65535 {
        out.put_u8(126);
        out.put_slice(&(len as u16).to_be_bytes());
    } else {
        out.put_u8(127);
        out.put_slice(&(len as u64).to_be_bytes());
    }

    out.put_slice(payload);
}

/// Queue one wire protocol message (opcode byte + payload) as a WS frame.
pub fn queue_message(client: &mut WsClient, code: u8, payload: &[u8]) {
    encode_wire_frame(code, payload, &mut client.send_queue);
}

/// Encode `[code][payload]` as an unmasked binary WS frame directly into
/// `out`, with no scratch buffer.
pub fn encode_wire_frame(code: u8, payload: &[u8], out: &mut impl BufMut) {
    let len = payload.len() + 1;
    out.put_u8(0x82);
    if len <= 125 {
        out.put_u8(len as u8);
    } else if len <= 65535 {
        out.put_u8(126);
        out.put_slice(&(len as u16).to_be_bytes());
    } else {
        out.put_u8(127);
        out.put_slice(&(len as u64).to_be_bytes());
    }
    out.put_u8(code);
    out.put_slice(payload);
}

/// Queue a WS control frame (ping/pong/close — always FIN, payload <= 125).
pub fn queue_ws_control(client: &mut WsClient, opcode: u8, payload: &[u8]) {
    client.send_queue.put_u8(0x80 | opcode);
    client.send_queue.put_u8(payload.len().min(125) as u8);
    client
        .send_queue
        .put_slice(&payload[..payload.len().min(125)]);
}

/// Queue a WS ping (heartbeat).
pub fn send_ws_ping(client: &mut WsClient) {
    queue_ws_control(client, 0x89, &[]);
    try_flush_client(client);
}

/// Queue a WS close frame with a Colyseus close code + reason.
pub fn queue_close_frame(client: &mut WsClient, code: u16, reason: &str) {
    let mut payload = Vec::with_capacity(2 + reason.len());
    payload.extend_from_slice(&code.to_be_bytes());
    payload.extend_from_slice(&reason.as_bytes()[..reason.len().min(123)]);
    queue_ws_control(client, 0x8, &payload);
}

/// Best-effort immediate write of queued bytes (non-blocking).
pub fn try_flush_client(client: &mut WsClient) {
    if client.send_queue.is_empty() {
        return;
    }
    match client.stream.write(&client.send_queue) {
        Ok(n) => {
            client.send_queue.advance(n);
        }
        Err(_) => {}
    }
}

/// Final write for a dying client: switch the socket to blocking so any queued
/// close frame is guaranteed on the wire, then drop (which closes the fd).
pub fn finish_close(client: WsClient) {
    if !client.send_queue.is_empty() {
        let std_stream: std::net::TcpStream = client.stream.into();
        let _ = std_stream.set_nonblocking(false);
        let _ = (&std_stream).write_all(&client.send_queue);
        // std_stream drops here, closing the socket.
    }
}
