
use std::io::Write;

use bytes::{Buf, BufMut, BytesMut};

use super::super::client::WsClient;

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

#[inline(always)]
pub fn queue_message(client: &mut WsClient, code: u8, payload: &[u8]) {
    encode_wire_frame(code, payload, &mut client.send_queue);
}

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

pub fn queue_ws_control(client: &mut WsClient, opcode: u8, payload: &[u8]) {
    client.send_queue.put_u8(0x80 | opcode);
    client.send_queue.put_u8(payload.len().min(125) as u8);
    client
        .send_queue
        .put_slice(&payload[..payload.len().min(125)]);
}

pub fn send_ws_ping(client: &mut WsClient) {
    queue_ws_control(client, 0x89, &[]);
    try_flush_client(client);
}

pub fn queue_close_frame(client: &mut WsClient, code: u16, reason: &str) {
    let mut payload = Vec::with_capacity(2 + reason.len());
    payload.extend_from_slice(&code.to_be_bytes());
    payload.extend_from_slice(&reason.as_bytes()[..reason.len().min(123)]);
    queue_ws_control(client, 0x8, &payload);
}

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

pub fn finish_close(client: WsClient) {
    if !client.send_queue.is_empty() {
        let std_stream: std::net::TcpStream = client.stream.into();
        let _ = std_stream.set_nonblocking(false);
        let _ = (&std_stream).write_all(&client.send_queue);

    }
}
