//! A hand-written RFC 6455 client: masked requests, unmasked replies. No WS
//! dependency exists in this crate's dev-deps, and the endpoint under test is
//! exactly a socket, so a raw one proves more than a mock would.

use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// The RFC 6455 sample key; the server's reply to it is never checked here.
pub(super) const WS_KEY: &str = "dGhlIHNhbXBsZSBub25jZQ==";

pub(super) fn upgrade_request(path: &str, addr: SocketAddr) -> String {
    format!(
        "GET {path} HTTP/1.1\r\nHost: {addr}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
         Sec-WebSocket-Key: {WS_KEY}\r\nSec-WebSocket-Version: 13\r\n\r\n"
    )
}

pub(super) async fn connect_io(addr: SocketAddr, path: &str) -> TcpStream {
    let mut io = tokio::time::timeout(AWAIT_TIMEOUT, TcpStream::connect(addr))
        .await
        .expect("connect timed out")
        .expect("connect failed");
    let req = upgrade_request(path, addr);
    tokio::time::timeout(AWAIT_TIMEOUT, io.write_all(req.as_bytes()))
        .await
        .expect("write timed out")
        .expect("write failed");
    io
}

pub(super) async fn read_byte(io: &mut TcpStream) -> Option<u8> {
    tokio::time::timeout(AWAIT_TIMEOUT, io.read_u8())
        .await
        .expect("socket stalled")
        .ok()
}

pub(super) async fn read_chunk(io: &mut TcpStream, buf: &mut [u8]) -> Option<()> {
    tokio::time::timeout(AWAIT_TIMEOUT, io.read_exact(buf))
        .await
        .expect("socket stalled")
        .ok()?;
    Some(())
}

/// Read response headers up to the blank line.
pub(super) async fn read_head(io: &mut TcpStream) -> Option<String> {
    let mut head = String::new();
    loop {
        head.push(read_byte(io).await? as char);
        if head.ends_with("\r\n\r\n") {
            return Some(head);
        }
    }
}

pub(super) struct WsClient {
    pub(super) io: TcpStream,
}

impl WsClient {
    /// Open a signaling session. Panics (fails the test) unless the server
    /// answered 101.
    pub(super) async fn open(addr: SocketAddr, path: &str) -> WsClient {
        let mut io = connect_io(addr, path).await;
        let head = read_head(&mut io).await.expect("no response head");
        assert!(head.starts_with("HTTP/1.1 101"), "upgrade refused: {head}");
        WsClient { io }
    }

    pub(super) async fn send_frame(&mut self, opcode: u8, payload: &[u8]) {
        let mut frame = vec![0x80 | opcode];
        if payload.len() < 126 {
            frame.push(0x80 | payload.len() as u8);
        } else if payload.len() < (1 << 16) {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        } else {
            frame.push(0x80 | 127);
            frame.extend_from_slice(&(payload.len() as u64).to_be_bytes());
        }
        let mask = [0x37u8, 0x38, 0x39, 0x3A];
        frame.extend_from_slice(&mask);
        frame.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        tokio::time::timeout(AWAIT_TIMEOUT, self.io.write_all(&frame))
            .await
            .expect("write timed out")
            .expect("write failed");
    }

    pub(super) async fn send_text(&mut self, msg: &str) {
        self.send_frame(0x1, msg.as_bytes()).await;
    }
}
