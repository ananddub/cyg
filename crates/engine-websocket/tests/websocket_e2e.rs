//! End-to-end WebSocket tests over real TCP sockets, with a hand-rolled
//! minimal client (masked frames like browsers send).

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};

use std::thread;
use std::time::Duration;

use engine_websocket::frame::{close_payload, encode_frame, opcode};
use engine_websocket::{WsConfig, WsEvent, WsServer};

fn spawn_server() -> SocketAddr {
    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let mut server = WsServer::new(addr, WsConfig::default()).unwrap();
    let bound = server.local_addr().unwrap();

    thread::spawn(move || loop {
        let events = server.poll_events(Duration::from_millis(50)).unwrap();
        for ev in events {
            match ev {
                WsEvent::Text { client_id, text } => {
                    if text == "close" {
                        server.close(&client_id, 1000, "bye").unwrap();
                    } else {
                        server.send_text(&client_id, &format!("echo:{text}")).unwrap();
                    }
                },
                WsEvent::Binary { client_id, data } => {
                    server.send_binary(&client_id, &data).unwrap();
                },
                _ => {},
            }
        }
    });
    bound
}

fn client_handshake(addr: SocketAddr) -> TcpStream {
    let mut sock = TcpStream::connect(addr).unwrap();
    sock.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    sock.write_all(
        b"GET /game HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n",
    )
    .unwrap();
    let mut resp = [0u8; 512];
    let n = sock.read(&mut resp).unwrap();
    let head = String::from_utf8_lossy(&resp[..n]).into_owned();
    assert!(head.starts_with("HTTP/1.1 101 Switching Protocols"), "got {head}");
    assert!(head.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo="));
    sock
}

fn send_masked(sock: &mut TcpStream, fin: bool, op: u8, payload: &[u8]) {
    let mut out = Vec::new();
    encode_frame(fin, op, payload, Some([0x37, 0xfa, 0x0c, 0x49]), &mut out);
    sock.write_all(&out).unwrap();
}

/// Read one unmasked server frame (blocking, with timeout).
fn recv_frame(sock: &mut TcpStream) -> (u8, Vec<u8>) {
    let mut b0 = [0u8; 1];
    sock.read_exact(&mut b0).unwrap();
    let opcode = b0[0] & 0x0F;
    let mut b1 = [0u8; 1];
    sock.read_exact(&mut b1).unwrap();
    assert_eq!(b1[0] & 0x80, 0, "server frames must not be masked");
    let mut len = (b1[0] & 0x7F) as usize;
    if len == 126 {
        let mut l = [0u8; 2];
        sock.read_exact(&mut l).unwrap();
        len = u16::from_be_bytes(l) as usize;
    }
    let mut payload = vec![0u8; len];
    if len > 0 {
        sock.read_exact(&mut payload).unwrap();
    }
    (opcode, payload)
}

#[test]
fn handshake_text_echo_and_close() {
    let addr = spawn_server();
    let mut sock = client_handshake(addr);

    send_masked(&mut sock, true, opcode::TEXT, b"hello");
    let (op, payload) = recv_frame(&mut sock);
    assert_eq!(op, opcode::TEXT);
    assert_eq!(payload, b"echo:hello");

    // Binary roundtrip
    send_masked(&mut sock, true, opcode::BINARY, &[1, 2, 3, 4]);
    let (op, payload) = recv_frame(&mut sock);
    assert_eq!(op, opcode::BINARY);
    assert_eq!(payload, vec![1, 2, 3, 4]);

    // Ping -> auto Pong
    send_masked(&mut sock, true, opcode::PING, b"hb");
    let (op, payload) = recv_frame(&mut sock);
    assert_eq!(op, opcode::PONG);
    assert_eq!(payload, b"hb");

    // Server-initiated close after text "close"
    send_masked(&mut sock, true, opcode::TEXT, b"close");
    let (op, payload) = recv_frame(&mut sock);
    assert_eq!(op, opcode::CLOSE);
    assert_eq!(&payload[..2], &1000u16.to_be_bytes());

    // Client echoes close back
    send_masked(&mut sock, true, opcode::CLOSE, &close_payload(1000, ""), );
    let _ = sock.read(&mut [0u8; 1]);
}

#[test]
fn fragmented_message_reassembles() {
    let addr = spawn_server();
    let mut sock = client_handshake(addr);

    send_masked(&mut sock, false, opcode::TEXT, b"fra");
    send_masked(&mut sock, true, opcode::CONTINUATION, b"g-123");
    let (op, payload) = recv_frame(&mut sock);
    assert_eq!(op, opcode::TEXT);
    assert_eq!(payload, b"echo:frag-123");
}

#[test]
fn non_ws_upgrade_is_rejected() {
    let addr = spawn_server();
    let mut sock = TcpStream::connect(addr).unwrap();
    sock.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    sock.write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
    let mut buf = Vec::new();
    sock.read_to_end(&mut buf).unwrap();
    let head = String::from_utf8_lossy(&buf).into_owned();
    assert!(head.starts_with("HTTP/1.1 400"), "got: {head}");
}

#[test]
fn client_initiated_close_gets_echo() {
    let addr = spawn_server();
    let mut sock = client_handshake(addr);
    send_masked(&mut sock, true, opcode::CLOSE, &close_payload(1000, "bye"));
    let (op, payload) = recv_frame(&mut sock);
    assert_eq!(op, opcode::CLOSE);
    assert_eq!(&payload[..2], &1000u16.to_be_bytes());
}
