//! Encoding server frames: binary messages, control frames and the two ways
//! queued bytes reach the socket.

use std::io::Write;

use bytes::{Buf, BufMut, BytesMut};

use super::super::client::WsClient;

/// Server frames are never masked (RFC 6455).
#[inline(always)]
pub fn encode_ws_frame(payload: &[u8], out: &mut impl BufMut) {
    let len = payload.len();
    if len <= 125 {
        let hdr = [0x82, len as u8];
        out.put_slice(&hdr);
    } else if len <= 65535 {
        let be = (len as u16).to_be_bytes();
        let hdr = [0x82, 126, be[0], be[1]];
        out.put_slice(&hdr);
    } else {
        let be = (len as u64).to_be_bytes();
        let mut hdr = [0u8; 10];
        hdr[0] = 0x82;
        hdr[1] = 127;
        hdr[2..10].copy_from_slice(&be);
        out.put_slice(&hdr);
    }
    out.put_slice(payload);
}

/// Queue one wire protocol message (opcode byte + payload) as a WS frame.
#[inline(always)]
pub fn queue_message(client: &mut WsClient, code: u8, payload: &[u8]) {
    encode_wire_frame(code, payload, &mut client.send_queue);
}

/// Encode `[code][payload]` as an unmasked binary WS frame directly into
/// `out`, with no scratch buffer.
#[inline(always)]
pub fn encode_wire_frame(code: u8, payload: &[u8], out: &mut BytesMut) {
    let len = payload.len() + 1;
    let total_len = if len <= 125 {
        len + 2
    } else if len <= 65535 {
        len + 4
    } else {
        len + 10
    };
    let cur_len = out.len();
    if out.capacity().saturating_sub(cur_len) < total_len {
        out.reserve(total_len);
    }
    unsafe {
        let ptr = out.as_mut_ptr().add(cur_len);
        if len <= 125 {
            *ptr = 0x82;
            *ptr.add(1) = len as u8;
            *ptr.add(2) = code;
            std::ptr::copy_nonoverlapping(payload.as_ptr(), ptr.add(3), payload.len());
        } else if len <= 65535 {
            *ptr = 0x82;
            *ptr.add(1) = 126;
            let be = (len as u16).to_be_bytes();
            *ptr.add(2) = be[0];
            *ptr.add(3) = be[1];
            *ptr.add(4) = code;
            std::ptr::copy_nonoverlapping(payload.as_ptr(), ptr.add(5), payload.len());
        } else {
            *ptr = 0x82;
            *ptr.add(1) = 127;
            let be = (len as u64).to_be_bytes();
            std::ptr::copy_nonoverlapping(be.as_ptr(), ptr.add(2), 8);
            *ptr.add(10) = code;
            std::ptr::copy_nonoverlapping(payload.as_ptr(), ptr.add(11), payload.len());
        }
        out.set_len(out.len() + total_len);
    }
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

#[cfg(test)]
mod boundary_tests {
    use super::*;

    #[test]
    fn test_encode_wire_frame_boundaries() {
        // Test empty payload (len = 1)
        let mut out = BytesMut::new();
        encode_wire_frame(15, &[], &mut out);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0], 0x82);
        assert_eq!(out[1], 1);
        assert_eq!(out[2], 15);

        // Test boundary len = 125 (payload = 124 bytes)
        out.clear();
        let p124 = vec![0xAA; 124];
        encode_wire_frame(15, &p124, &mut out);
        assert_eq!(out.len(), 124 + 3);
        assert_eq!(out[0], 0x82);
        assert_eq!(out[1], 125);
        assert_eq!(out[2], 15);
        assert_eq!(&out[3..], &p124[..]);

        // Test boundary len = 126 (payload = 125 bytes) -> transitions to 2-byte len
        out.clear();
        let p125 = vec![0xBB; 125];
        encode_wire_frame(15, &p125, &mut out);
        assert_eq!(out.len(), 125 + 5);
        assert_eq!(out[0], 0x82);
        assert_eq!(out[1], 126);
        assert_eq!(&out[2..4], &126u16.to_be_bytes());
        assert_eq!(out[4], 15);
        assert_eq!(&out[5..], &p125[..]);

        // Test boundary len = 65535 (payload = 65534 bytes)
        out.clear();
        let p65534 = vec![0xCC; 65534];
        encode_wire_frame(15, &p65534, &mut out);
        assert_eq!(out.len(), 65534 + 5);
        assert_eq!(out[0], 0x82);
        assert_eq!(out[1], 126);
        assert_eq!(&out[2..4], &65535u16.to_be_bytes());
        assert_eq!(out[4], 15);

        // Test boundary len = 65536 (payload = 65535 bytes) -> transitions to 8-byte len
        out.clear();
        let p65535 = vec![0xDD; 65535];
        encode_wire_frame(15, &p65535, &mut out);
        assert_eq!(out.len(), 65535 + 11);
        assert_eq!(out[0], 0x82);
        assert_eq!(out[1], 127);
        assert_eq!(&out[2..10], &65536u64.to_be_bytes());
        assert_eq!(out[10], 15);
    }
}
