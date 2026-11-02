
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use engine_net::transport::{GameTransport, TransportEvent, WsTransport};

const SERIALIZER_ID: &[u8] = b"schema";

fn spawn_transport() -> (WsTransport, u16) {
    let transport = WsTransport::new("127.0.0.1:0".parse().unwrap()).expect("bind ok");
    let port = transport.local_addr().unwrap().port();
    (transport, port)
}

fn poll(transport: &mut WsTransport) -> Vec<TransportEvent> {

    let _ = transport.flush();
    let mut events = Vec::new();
    transport
        .poll_events(Duration::from_millis(50), &mut events)
        .expect("poll ok");
    events
}

fn send_ws_frame(stream: &mut TcpStream, payload: &[u8]) {
    let mask = [0x11u8, 0x22, 0x33, 0x44];
    let mut frame = vec![0x82];
    if payload.len() <= 125 {
        frame.push(0x80 | payload.len() as u8);
    } else {
        frame.push(0x80 | 126);
        frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    }
    frame.extend_from_slice(&mask);
    frame.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
    stream.write_all(&frame).expect("frame write ok");
}

fn read_ws_frame(stream: &mut TcpStream) -> Vec<u8> {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut header = [0u8; 2];
    stream.read_exact(&mut header).expect("frame header");
    assert_eq!(header[0] & 0x0F, 0x02, "must be a binary frame");
    let len = (header[1] & 0x7F) as usize;
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).expect("frame payload");
    payload
}

fn ws_upgrade(port: u16, transport: &mut WsTransport) -> TcpStream {
    ws_upgrade_with_query(port, transport, "sessionId=sess_1")
}

fn ws_upgrade_with_query(port: u16, transport: &mut WsTransport, query: &str) -> TcpStream {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect ok");

    let request = format!(
        "GET /cyg_1/game_room?{query} HTTP/1.1\r\n\
         Host: localhost\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Version: 13\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).expect("upgrade write");

    
    let mut response = String::new();
    let mut byte = [0u8; 1];
    stream
        .set_read_timeout(Some(Duration::from_millis(20)))
        .unwrap();
    loop {
        match stream.read(&mut byte) {
            Ok(1) => {
                response.push(byte[0] as char);
                if response.ends_with("\r\n\r\n") {
                    break;
                }
            }
            _ => {
                poll(transport); 
            }
        }
    }
    
    poll(transport);

    assert!(
        response.contains("101 Switching Protocols"),
        "expected 101, got: {response}"
    );
    assert!(
        response.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo="),
        "wrong Sec-WebSocket-Accept (RFC 6455 test vector), got: {response}"
    );
    stream
}

#[test]
fn test_matchmake_seat_reservation_http() {
    let (mut transport, port) = spawn_transport();

    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect ok");
    let request = "POST /matchmake/joinOrCreate/game_room HTTP/1.1\r\n\
                   Host: localhost\r\n\
                   Content-Type: application/json\r\n\
                   Content-Length: 2\r\n\r\n{}";
    stream.write_all(request.as_bytes()).expect("post ok");

    
    stream
        .set_read_timeout(Some(Duration::from_millis(20)))
        .unwrap();
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(_) => {
                poll(&mut transport);
            }
        }
    }
    let response = String::from_utf8_lossy(&buf).into_owned();
    assert!(
        response.contains("200 OK"),
        "seat reservation must be 200: {response}"
    );
    
    assert!(response.contains("\"sessionId\":\""), "missing sessionId");
    assert!(
        response.contains("\"roomId\":\"game_room\""),
        "missing roomId"
    );
    assert!(response.contains("\"processId\":\""), "missing processId");
    assert!(response.contains("\"name\":\"game_room\""), "missing name");

    let events = poll(&mut transport);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, TransportEvent::Connected { .. })),
        "matchmake HTTP client must not register as a WS connection"
    );
}

#[test]
fn test_join_room_reply_matches_sdk_wire_format() {
    let (mut transport, port) = spawn_transport();
    let mut stream = ws_upgrade(port, &mut transport);

    let reply = read_ws_frame(&mut stream);

    assert_eq!(
        reply[0], 10,
        "first byte must be JOIN_ROOM (10), not ERROR (11)"
    );
    let rt_len = reply[1] as usize;
    assert!(rt_len > 0, "reconnectionToken must be non-empty");
    let ser_len = reply[2 + rt_len] as usize;
    assert_eq!(ser_len, SERIALIZER_ID.len(), "serializer length prefix");
    let serializer = &reply[3 + rt_len..3 + rt_len + ser_len];
    assert_eq!(serializer, SERIALIZER_ID, "serializerId must be 'schema'");
    assert_eq!(
        reply[3 + rt_len + ser_len],
        0x00,
        "empty state reflection must be msgpack fixint 0"
    );
}

#[test]
fn test_room_data_reaches_simulation_and_leave_disconnects() {
    let (mut transport, port) = spawn_transport();
    let mut stream = ws_upgrade(port, &mut transport);

    send_ws_frame(&mut stream, &[13, b'm', b'o', b'v', b'e']);
    let mut events = poll(&mut transport);
    for _ in 0..10 {
        if events
            .iter()
            .any(|e| matches!(e, TransportEvent::Packet { .. }))
        {
            break;
        }
        events = poll(&mut transport);
    }
    assert!(events.iter().any(
        |e| matches!(e, TransportEvent::Packet { opcode: engine_net::framing::OpCode::PlayerInput, payload, .. }
            if payload.as_ref() == b"move")
    ));

    send_ws_frame(&mut stream, &[12]);
    let mut events = poll(&mut transport);
    for _ in 0..10 {
        if events
            .iter()
            .any(|e| matches!(e, TransportEvent::Disconnected { .. }))
        {
            break;
        }
        events = poll(&mut transport);
    }
    assert!(
        events
            .iter()
            .any(|e| matches!(e, TransportEvent::Disconnected { .. })),
        "LEAVE_ROOM must disconnect the client"
    );
    assert!(
        transport.take_consented("sess_1"),
        "LEAVE_ROOM is an intentional departure: the seat must not be held"
    );
}

#[test]
fn test_socket_drop_is_not_consented() {
    let (mut transport, port) = spawn_transport();
    let stream = ws_upgrade(port, &mut transport);

    drop(stream);

    let mut events = poll(&mut transport);
    for _ in 0..10 {
        if events
            .iter()
            .any(|e| matches!(e, TransportEvent::Disconnected { .. }))
        {
            break;
        }
        events = poll(&mut transport);
    }
    assert!(
        events
            .iter()
            .any(|e| matches!(e, TransportEvent::Disconnected { .. })),
        "a dropped socket must surface a disconnect"
    );
    assert!(
        !transport.take_consented("sess_1"),
        "an unexpected drop must stay eligible for allowReconnection"
    );
}

#[test]
fn test_join_options_are_percent_decoded_for_auth() {
    let (mut transport, port) = spawn_transport();
    let _stream = ws_upgrade_with_query(
        port,
        &mut transport,
        "sessionId=sess_opt&options=%7B%22token%22%3A%22a+b%22%7D",
    );

    assert_eq!(
        transport.options_of("sess_opt"),
        Some(r#"{"token":"a b"}"#),
        "the host must see the same JSON the browser serialized"
    );
    assert!(
        transport
            .room_of("sess_opt")
            .is_some_and(|room| room.starts_with("game_room")),
        "the room id travels with the upgrade URL"
    );
}
