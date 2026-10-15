//! Feature-completeness tests for the Colyseus wire-protocol transport,
//! covering everything beyond the basic join/data/leave flow:
//! - App-level PING (18) echo — powers `room.ping()` and `client.getLatency()`
//! - RFC 6455 control frames: server answers WS pings, echoes close frames
//! - ROOM_DATA_BYTES (17) dispatch as `OpCode::RoomDataBytes`
//! - ROOM_REQUEST (21) / ROOM_RESPONSE (22) RPC round-trip
//! - ERROR (11) wire format (`room.onError` payload)
//! - Message fragmentation (RFC 6455 continuation frames)
//! - Protocol modifier-bit stripping (`code = byte & 0x1F`)
//! - Matchmake method whitelist + typed MatchMakeError codes
//! - Default reconnect flow via `retain_session`
//! - JOIN_ROOM handshake echoing the seat's reconnectionToken
//! - Opcode mapping: ROOM_STATE (14) vs ROOM_STATE_PATCH (15)
//! - Server-initiated disconnect with close code 4000 (CONSENTED)
//! - Unresponsive-client heartbeat termination
//!
//! Protocol values verified against `packages/shared-types/src/Protocol.ts`
//! and `packages/sdk/src/Room.ts` of the OpenOxide wire (0.18-derived) monorepo.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use engine_net::framing::OpCode;
use engine_net::transport::ws::{HeartbeatConfig, MatchMakeError};
use engine_net::transport::{WsTransport, GameTransport, TransportEvent};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn spawn_transport() -> (WsTransport, u16) {
    let transport = WsTransport::new("127.0.0.1:0".parse().unwrap()).expect("bind ok");
    let port = transport.local_addr().unwrap().port();
    (transport, port)
}

fn poll(transport: &mut WsTransport) -> Vec<TransportEvent> {
    let mut events = Vec::new();
    transport
        .poll_events(Duration::from_millis(30), &mut events)
        .expect("poll ok");
    events
}

/// Poll until `pred` matches an event, or panic after ~2s.
fn poll_until(
    transport: &mut WsTransport,
    pred: impl Fn(&TransportEvent) -> bool,
) -> Vec<TransportEvent> {
    for _ in 0..60 {
        let events = poll(transport);
        if events.iter().any(&pred) {
            return events;
        }
    }
    panic!("condition not met within polling window");
}

/// Send one masked RFC 6455 frame with a configurable first byte
/// (FIN + opcode). Client frames MUST be masked.
fn send_ws_frame_raw(stream: &mut TcpStream, first_byte: u8, payload: &[u8]) {
    let mask = [0x11u8, 0x22, 0x33, 0x44];
    let mut frame = vec![first_byte];
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

/// Send one masked binary data frame (FIN=1, opcode 0x2).
fn send_ws_frame(stream: &mut TcpStream, payload: &[u8]) {
    send_ws_frame_raw(stream, 0x82, payload);
}

/// Read one server frame (any opcode, unmasked server frames), pumping the
/// transport while waiting: server replies are only written during
/// poll/flush, so a plain blocking read would starve.
/// Returns `(ws_opcode, payload)`.
fn read_frame(stream: &mut TcpStream, transport: &mut WsTransport) -> (u8, Vec<u8>) {
    stream
        .set_read_timeout(Some(Duration::from_millis(20)))
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    let mut buf = Vec::new();
    loop {
        if let Some(frame) = try_parse_ws_frame(&buf) {
            return frame;
        }
        if std::time::Instant::now() > deadline {
            panic!("no server frame within window; buffered: {buf:02X?}");
        }
        let mut chunk = [0u8; 4096];
        match stream.read(&mut chunk) {
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(ref e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                let _ = poll(transport);
                let _ = transport.flush();
            }
            Err(e) => panic!("socket read error: {e}"),
        }
    }
}

/// Parse the first complete frame out of a byte buffer (server frames are
/// unmasked), if one is fully buffered.
fn try_parse_ws_frame(buf: &[u8]) -> Option<(u8, Vec<u8>)> {
    if buf.len() < 2 {
        return None;
    }
    let ws_opcode = buf[0] & 0x0F;
    let len = (buf[1] & 0x7F) as usize;
    let (header_len, payload_len) = match len {
        126 => {
            if buf.len() < 4 {
                return None;
            }
            (4usize, u16::from_be_bytes([buf[2], buf[3]]) as usize)
        }
        127 => {
            if buf.len() < 10 {
                return None;
            }
            let n = u64::from_be_bytes(buf[2..10].try_into().unwrap()) as usize;
            (10usize, n)
        }
        n => (2usize, n),
    };
    if buf.len() < header_len + payload_len {
        return None;
    }
    Some((ws_opcode, buf[header_len..header_len + payload_len].to_vec()))
}

/// Complete the RFC 6455 upgrade and return the stream (handshake reply +
/// JOIN_ROOM frame are drained).
fn ws_upgrade(port: u16, transport: &mut WsTransport, path: &str) -> TcpStream {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect ok");
    let request = format!(
        "GET {} HTTP/1.1\r\n\
         Host: localhost\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Version: 13\r\n\r\n",
        path
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
    assert!(
        response.contains("101 Switching Protocols"),
        "expected 101, got: {response}"
    );
    // Drain the proactive JOIN_ROOM handshake so later reads see app frames.
    let _ = read_frame(&mut stream, transport);
    stream
}

/// POST a matchmake request; returns the full HTTP response string.
fn post_matchmake(transport: &mut WsTransport, port: u16, path: &str, body: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect ok");
    let request = format!(
        "POST {} HTTP/1.1\r\n\
         Host: localhost\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\r\n{}",
        path,
        body.len(),
        body
    );
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
                poll(transport);
            }
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

/// Tiny JSON string-field extractor for seat-reservation responses.
fn json_field(body: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\"", key);
    let idx = body.find(&needle)?;
    let rest = body[idx + needle.len()..].strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// msgpack decode of a small unsigned integer (fixint / uint8 / uint16).
fn decode_mp_uint(bytes: &[u8]) -> Option<u64> {
    match bytes.first() {
        Some(b) if *b < 0x80 => Some(*b as u64),
        Some(0xcc) => bytes.get(1).map(|v| *v as u64),
        Some(0xcd) => bytes
            .get(1..3)
            .map(|v| u16::from_le_bytes(v.try_into().unwrap()) as u64),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// App-level PING (18)
// ---------------------------------------------------------------------------

#[test]
fn test_ping_opcode_echoed() {
    let (mut transport, port) = spawn_transport();
    let mut stream = ws_upgrade(port, &mut transport, "/openoxide_1/game_room?sessionId=sess_ping");

    // SDK room.ping() / getLatency() send a bare [18] binary message.
    send_ws_frame(&mut stream, &[18]);
    transport.flush().expect("flush");

    let (ws_op, payload) = read_frame(&mut stream, &mut transport);
    assert_eq!(ws_op, 0x2, "PING echo must be a binary frame");
    assert_eq!(payload, vec![18], "PING (18) must be echoed verbatim");
}

#[test]
fn test_ws_control_ping_answered_with_pong() {
    let (mut transport, port) = spawn_transport();
    let mut stream = ws_upgrade(port, &mut transport, "/openoxide_1/game_room?sessionId=sess_ws");

    // RFC 6455 ping (opcode 0x9) with an application payload.
    send_ws_frame_raw(&mut stream, 0x89, b"probe");
    transport.flush().expect("flush");

    let (ws_op, payload) = read_frame(&mut stream, &mut transport);
    assert_eq!(ws_op, 0xA, "WS ping must be answered with a pong");
    assert_eq!(payload, b"probe", "pong must echo the ping payload");
}

// ---------------------------------------------------------------------------
// ROOM_DATA_BYTES / ROOM_REQUEST / ERROR dispatch & replies
// ---------------------------------------------------------------------------

#[test]
fn test_room_data_bytes_dispatch() {
    let (mut transport, port) = spawn_transport();
    let mut stream = ws_upgrade(port, &mut transport, "/openoxide_1/game_room?sessionId=sess_rdb");

    // SDK room.sendBytes('input', bytes): [17][msgpack str "input"][bytes...]
    let mut msg = vec![17u8, 0xa5];
    msg.extend_from_slice(b"input");
    msg.push(8); // FLAG_D
    send_ws_frame(&mut stream, &msg);

    let events = poll_until(&mut transport, |e| {
        matches!(e, TransportEvent::Packet { opcode: OpCode::RoomDataBytes, .. })
    });
    let TransportEvent::Packet { opcode, payload, .. } = events
        .iter()
        .find(|e| matches!(e, TransportEvent::Packet { opcode: OpCode::RoomDataBytes, .. }))
        .unwrap()
    else {
        unreachable!()
    };
    assert_eq!(*opcode, OpCode::RoomDataBytes);
    // Protocol byte (17) stripped: payload = [msgpack "input"][flags]
    assert_eq!(payload[..6], [0xa5, b'i', b'n', b'p', b'u', b't']);
    assert_eq!(payload[6], 8);
}

#[test]
fn test_room_request_and_response_roundtrip() {
    let (mut transport, port) = spawn_transport();
    let mut stream = ws_upgrade(port, &mut transport, "/openoxide_1/game_room?sessionId=sess_rpc");

    // SDK room.request(): [21][msgpack requestId][msgpackr payload]
    send_ws_frame(&mut stream, &[21, 7, 0xa3, b'h', b'i', b'!']);

    let events = poll_until(&mut transport, |e| {
        matches!(e, TransportEvent::Packet { opcode: OpCode::RoomRequest, .. })
    });
    let TransportEvent::Packet { client_id, payload, .. } = events
        .iter()
        .find(|e| matches!(e, TransportEvent::Packet { opcode: OpCode::RoomRequest, .. }))
        .unwrap()
    else {
        unreachable!()
    };
    assert_eq!(payload[0], 7, "request id must be forwarded to the game layer");

    // Game replies: [22][msgpack requestId][status][payload]
    transport
        .send_room_response(client_id, 7, engine_net::response_status::OK, &[0xa3, b'y', b'e', b's'])
        .expect("response");
    transport.flush().expect("flush");

    let (ws_op, payload) = read_frame(&mut stream, &mut transport);
    assert_eq!(ws_op, 0x2);
    assert_eq!(payload[0], 22, "ROOM_RESPONSE opcode");
    assert_eq!(payload[1], 7, "request id echo (msgpack fixint)");
    assert_eq!(payload[2], 0, "ResponseStatus::OK");
    assert_eq!(&payload[3..], &[0xa3, b'y', b'e', b's']);
}

// ---------------------------------------------------------------------------
// Fragmentation & modifier bits
// ---------------------------------------------------------------------------

#[test]
fn test_fragmented_message_dispatched_once() {
    let (mut transport, port) = spawn_transport();
    let mut stream = ws_upgrade(port, &mut transport, "/openoxide_1/game_room?sessionId=sess_frag");

    // ROOM_DATA split across two frames: first (FIN=0, binary), then a
    // continuation (FIN=1, opcode 0).
    send_ws_frame_raw(&mut stream, 0x02, &[13, b'h', b'e']);
    send_ws_frame_raw(&mut stream, 0x80, &[b'l', b'l', b'o']);

    let events = poll_until(&mut transport, |e| {
        matches!(e, TransportEvent::Packet { opcode: OpCode::PlayerInput, .. })
    });
    let packets: Vec<&TransportEvent> = events
        .iter()
        .filter(|e| matches!(e, TransportEvent::Packet { opcode: OpCode::PlayerInput, .. }))
        .collect();
    assert_eq!(packets.len(), 1, "fragments must reassemble into ONE message");
    if let TransportEvent::Packet { payload, .. } = packets[0] {
        assert_eq!(payload, &vec![b'h', b'e', b'l', b'l', b'o']);
    }
}

#[test]
fn test_modifier_bits_stripped_before_dispatch() {
    let (mut transport, port) = spawn_transport();
    let mut stream = ws_upgrade(port, &mut transport, "/openoxide_1/game_room?sessionId=sess_mod");

    // JOIN_ROOM ack decorated with the TIMED modifier bit (0x80): the SDK
    // strips modifier bits before dispatch; so must we.
    send_ws_frame(&mut stream, &[10 | 0x80]);
    let events = poll(&mut transport);
    assert!(
        !events.iter().any(|e| matches!(e, TransportEvent::Packet { .. })),
        "JOIN_ROOM ack (with modifier bits) must not surface as a packet"
    );
    assert!(
        !events.iter().any(|e| matches!(e, TransportEvent::Disconnected { .. })),
        "JOIN_ROOM ack must not disconnect"
    );

    // ROOM_DATA with the UNRELIABLE modifier (0x40) still delivers.
    send_ws_frame(&mut stream, &[13 | 0x40, 42]);
    let events = poll_until(&mut transport, |e| {
        matches!(e, TransportEvent::Packet { opcode: OpCode::PlayerInput, .. })
    });
    if let Some(TransportEvent::Packet { payload, .. }) = events
        .iter()
        .find(|e| matches!(e, TransportEvent::Packet { opcode: OpCode::PlayerInput, .. }))
    {
        assert_eq!(payload, &vec![42]);
    }
}

// ---------------------------------------------------------------------------
// Matchmake: whitelist, typed errors, default reconnect
// ---------------------------------------------------------------------------

#[test]
fn test_matchmake_whitelist_all_methods() {
    let (mut transport, port) = spawn_transport();

    for method in ["create", "join", "joinById", "joinOrCreate"] {
        let response = post_matchmake(
            &mut transport,
            port,
            &format!("/matchmake/{}/arena", method),
            "{}",
        );
        assert!(response.contains("200 OK"), "{method}: {response}");
        assert!(response.contains("\"sessionId\""), "{method}: {response}");
    }

    // Unknown method -> MATCHMAKE_NO_HANDLER (520).
    let response = post_matchmake(
        &mut transport,
        port,
        "/matchmake/summon/game_room",
        "{}",
    );
    assert!(
        response.contains("\"code\":520"),
        "unknown method must return 520, got: {response}"
    );
}

#[test]
fn test_matchmake_hook_typed_errors() {
    let (mut transport, port) = spawn_transport();
    transport.on_matchmake(move |method, room, _body| {
        if room == "locked" {
            Err(MatchMakeError::auth_failed("invite only"))
        } else if method == "joinById" {
            Err(MatchMakeError::invalid_room_id("no such room"))
        } else {
            Ok(format!(
                "{{\"name\":\"{room}\",\"sessionId\":\"s1\",\"roomId\":\"{room}\",\"processId\":\"p\"}}"
            ))
        }
    });

    let response = post_matchmake(&mut transport, port, "/matchmake/joinOrCreate/locked", "{}");
    assert!(
        response.contains("\"code\":525"),
        "AUTH_FAILED is 525, got: {response}"
    );

    let response = post_matchmake(&mut transport, port, "/matchmake/joinById/ghost", "{}");
    assert!(
        response.contains("\"code\":522"),
        "INVALID_ROOM_ID is 522, got: {response}"
    );

    let response = post_matchmake(&mut transport, port, "/matchmake/joinOrCreate/open", "{}");
    assert!(response.contains("200 OK"), "hook Ok path: {response}");
}

#[test]
fn test_default_reconnect_requires_retained_session() {
    let (mut transport, port) = spawn_transport();

    // Unknown/expired token -> MATCHMAKE_EXPIRED (524).
    let response = post_matchmake(
        &mut transport,
        port,
        "/matchmake/reconnect/arena",
        "{\"reconnectionToken\":\"arena:ghost\"}",
    );
    assert!(
        response.contains("\"code\":524"),
        "expired token must be 524, got: {response}"
    );

    // After retain_session, the same token re-issues the seat.
    transport.retain_session("arena", "sess_back");
    let response = post_matchmake(
        &mut transport,
        port,
        "/matchmake/reconnect/arena",
        "{\"reconnectionToken\":\"arena:sess_back\"}",
    );
    assert!(response.contains("200 OK"), "{response}");
    assert!(
        response.contains("\"sessionId\":\"sess_back\""),
        "reconnect must restore the SAME session: {response}"
    );
}

#[test]
fn test_join_handshake_echoes_seat_reconnection_token() {
    let (mut transport, port) = spawn_transport();

    // 1. Matchmake a seat (default hook) and extract its fields.
    let seat = post_matchmake(&mut transport, port, "/matchmake/joinOrCreate/game_room", "{}");
    let session = json_field(&seat, "sessionId").expect("sessionId in seat");
    let token = json_field(&seat, "reconnectionToken").expect("reconnectionToken in seat");
    assert_eq!(token, format!("game_room:{}", session));

    // 2. Consume the seat: WS connect with the sessionId query param.
    let stream = ws_upgrade(
        port,
        &mut transport,
        &format!("/openoxide_1/game_room?sessionId={}", session),
    );

    // The JOIN_ROOM handshake was drained by ws_upgrade — reconnect with a
    // fresh client to inspect it.
    drop(stream);
    let mut stream2 = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let request = format!(
        "GET /openoxide_1/game_room?sessionId={} HTTP/1.1\r\n\
         Host: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n",
        session
    );
    stream2.write_all(request.as_bytes()).unwrap();
    let mut response = String::new();
    let mut byte = [0u8; 1];
    stream2
        .set_read_timeout(Some(Duration::from_millis(20)))
        .unwrap();
    loop {
        match stream2.read(&mut byte) {
            Ok(1) => {
                response.push(byte[0] as char);
                if response.ends_with("\r\n\r\n") {
                    break;
                }
            }
            _ => {
                poll(&mut transport);
            }
        }
    }
    let (_ws_op, reply) = read_frame(&mut stream2, &mut transport);

    // JOIN_ROOM: [10][rt-len][rt][serializer-len]["schema"][0x00]
    assert_eq!(reply[0], 10);
    let rt_len = reply[1] as usize;
    let rt = String::from_utf8(reply[2..2 + rt_len].to_vec()).unwrap();
    assert_eq!(
        rt, token,
        "handshake token must equal the seat's reconnectionToken"
    );
}

// ---------------------------------------------------------------------------
// Opcode mapping & server-initiated disconnect
// ---------------------------------------------------------------------------

#[test]
fn test_opcode_mapping_state_snapshot_vs_patch() {
    let (mut transport, port) = spawn_transport();
    let mut stream = ws_upgrade(port, &mut transport, "/openoxide_1/game_room?sessionId=sess_map");

    // Connected already fired inside ws_upgrade's polling; the session id is
    // the upgrade URL's sessionId param.
    let cid = "sess_map".to_string();

    transport.send_to(&cid, OpCode::StateSnapshot, &[0xAA]).unwrap();
    transport.flush().unwrap();
    let (_, payload) = read_frame(&mut stream, &mut transport);
    assert_eq!(payload[0], 14, "StateSnapshot must map to ROOM_STATE (14)");
    assert_eq!(payload[1], 0xAA);

    transport.send_to(&cid, OpCode::StateDelta, &[0xBB]).unwrap();
    transport.flush().unwrap();
    let (_, payload) = read_frame(&mut stream, &mut transport);
    assert_eq!(payload[0], 15, "StateDelta must map to ROOM_STATE_PATCH (15)");
    assert_eq!(payload[1], 0xBB);
}

#[test]
fn test_server_disconnect_close_code_4000() {
    let (mut transport, port) = spawn_transport();
    let mut stream = ws_upgrade(port, &mut transport, "/openoxide_1/game_room?sessionId=sess_kick");

    // Connected already fired inside ws_upgrade's polling.
    let cid = "sess_kick".to_string();

    transport.disconnect(&cid).expect("disconnect");

    // Client receives a WS close frame carrying CloseCode::CONSENTED (4000).
    let (ws_op, payload) = read_frame(&mut stream, &mut transport);
    assert_eq!(ws_op, 0x8, "expected a close frame");
    let code = u16::from_be_bytes([payload[0], payload[1]]);
    assert_eq!(code, 4000, "server disconnect must use CONSENTED (4000)");

    // The Disconnected event surfaces on the next poll.
    poll_until(&mut transport, |e| {
        matches!(e, TransportEvent::Disconnected { client_id } if &**client_id == &*cid)
    });
}

#[test]
fn test_leave_room_answers_close_code_4000() {
    let (mut transport, port) = spawn_transport();
    let mut stream = ws_upgrade(port, &mut transport, "/openoxide_1/game_room?sessionId=sess_lv");

    // SDK Room.leave(): sends the 1-byte [12] message before closing.
    send_ws_frame(&mut stream, &[12]);

    let events = poll_until(&mut transport, |e| {
        matches!(e, TransportEvent::Disconnected { .. })
    });
    assert!(events
        .iter()
        .any(|e| matches!(e, TransportEvent::Disconnected { client_id } if &**client_id == "sess_lv")));

    // Server must also answer with a close frame on the wire.
    let (ws_op, payload) = read_frame(&mut stream, &mut transport);
    assert_eq!(ws_op, 0x8, "expected close frame after LEAVE_ROOM");
    let code = u16::from_be_bytes([payload[0], payload[1]]);
    assert_eq!(code, 4000, "LEAVE_ROOM close code must be CONSENTED (4000)");
}

// ---------------------------------------------------------------------------
// Heartbeat
// ---------------------------------------------------------------------------

#[test]
fn test_heartbeat_terminates_unresponsive_client() {
    let (mut transport, port) = spawn_transport();
    transport.set_heartbeat(HeartbeatConfig {
        enabled: true,
        ping_interval: Duration::from_millis(30),
        max_retries: 1,
    });

    // Connect, then go completely silent (no pongs, no data).
    let _stream = ws_upgrade(port, &mut transport, "/openoxide_1/game_room?sessionId=sess_dead");

    let start = std::time::Instant::now();
    poll_until(&mut transport, |e| {
        matches!(e, TransportEvent::Disconnected { client_id } if &**client_id == "sess_dead")
    });
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "unresponsive client must be terminated quickly"
    );
}

#[test]
fn test_heartbeat_spares_active_client() {
    let (mut transport, port) = spawn_transport();
    transport.set_heartbeat(HeartbeatConfig {
        enabled: true,
        ping_interval: Duration::from_millis(30),
        max_retries: 1,
    });

    let mut stream = ws_upgrade(port, &mut transport, "/openoxide_1/game_room?sessionId=sess_live");

    // Keep the client active: app-level [18] pings every iteration. Any
    // inbound frame proves liveness, so the heartbeat must never fire.
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_millis(300) {
        send_ws_frame(&mut stream, &[18]);
        for event in poll(&mut transport) {
            assert!(
                !matches!(&event, TransportEvent::Disconnected { client_id } if &**client_id == "sess_live"),
                "active client must not be terminated by the heartbeat"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// msgpack sanity (used by ERROR / ROOM_RESPONSE wire formats)
// ---------------------------------------------------------------------------

#[test]
fn test_error_frame_wire_format() {
    let (mut transport, port) = spawn_transport();
    let mut stream = ws_upgrade(port, &mut transport, "/openoxide_1/game_room?sessionId=sess_err2");

    // Connected already fired inside ws_upgrade's polling.
    let cid = "sess_err2".to_string();

    transport.send_error(&cid, 4217, "boom").unwrap();
    transport.flush().unwrap();

    // [11][msgpack 4217][msgpack "boom"]; @colyseus/schema numbers are
    // little-endian, so uint16 4217 (0x1079) encodes as 0xcd 0x79 0x10.
    let (_, payload) = read_frame(&mut stream, &mut transport);
    assert_eq!(payload[0], 11, "ERROR opcode");
    assert_eq!(decode_mp_uint(&payload[1..4]), Some(4217));
    assert_eq!(&payload[4..], &[0xa4, b'b', b'o', b'o', b'm'], "msgpack 'boom'");
}
