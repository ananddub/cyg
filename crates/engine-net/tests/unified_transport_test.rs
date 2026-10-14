use std::io::{Read, Write};
use std::net::{TcpStream, UdpSocket};
use std::time::Duration;

use engine_net::{
    OpCode, TransportEvent, TransportType, create_transport, decode_frame, encode_frame,
};

#[test]
fn test_unified_transport_auto_switch() {
    println!("\n==================================================================");
    println!("     UNIFIED TRANSPORT TRAIT AUTO-SWITCHING VERIFICATION TEST     ");
    println!("==================================================================");

    // -------------------------------------------------------------------------
    // TEST 1: Raw TCP Transport via GameTransport Trait
    // -------------------------------------------------------------------------
    let tcp_addr = "127.0.0.1:19410".parse().unwrap();
    let mut tcp_transport =
        create_transport(TransportType::Tcp, tcp_addr).expect("Create TCP transport ok");
    assert_eq!(tcp_transport.transport_name(), "Raw TCP");
    println!("✓ 1. Transport created: {}", tcp_transport.transport_name());

    let mut tcp_client = TcpStream::connect(tcp_addr).expect("TCP connect ok");
    tcp_client.set_nodelay(true).unwrap();

    let mut events = Vec::new();
    tcp_transport
        .poll_events(Duration::from_millis(50), &mut events)
        .unwrap();
    let connected_event = events
        .iter()
        .any(|e| matches!(e, TransportEvent::Connected { .. }));
    assert!(connected_event, "TCP client connected event dispatched");

    // Client sends PlayerInput packet
    let mut send_buf = Vec::new();
    encode_frame(OpCode::PlayerInput, b"move_forward", &mut send_buf);
    tcp_client.write_all(&send_buf).unwrap();

    events.clear();
    tcp_transport
        .poll_events(Duration::from_millis(50), &mut events)
        .unwrap();
    let packet_received = events.iter().any(|e| matches!(e, TransportEvent::Packet { opcode, payload, .. } if *opcode == OpCode::PlayerInput && payload.as_ref() == b"move_forward"));
    assert!(packet_received, "TCP packet received through trait");

    // Server broadcasts StateDelta
    tcp_transport
        .broadcast(OpCode::StateDelta, b"state_pos_100")
        .unwrap();
    tcp_transport.flush().unwrap();

    let mut recv_buf = [0u8; 1024];
    let n = tcp_client.read(&mut recv_buf).unwrap();
    let mut temp = recv_buf[..n].to_vec();
    let (op, payload) = decode_frame(&mut temp).unwrap();
    assert_eq!(op, OpCode::StateDelta);
    assert_eq!(payload, b"state_pos_100");
    println!("✓ 2. Raw TCP Transport roundtrip verified via GameTransport trait!");

    // -------------------------------------------------------------------------
    // TEST 2: Raw UDP Transport via GameTransport Trait
    // -------------------------------------------------------------------------
    let udp_addr = "127.0.0.1:19420".parse().unwrap();
    let mut udp_transport =
        create_transport(TransportType::Udp, udp_addr).expect("Create UDP transport ok");
    assert_eq!(udp_transport.transport_name(), "Raw UDP");
    println!("✓ 3. Transport created: {}", udp_transport.transport_name());

    let client_udp = UdpSocket::bind("127.0.0.1:0").unwrap();
    let mut udp_send = Vec::new();
    encode_frame(OpCode::PlayerInput, b"udp_shoot", &mut udp_send);
    client_udp.send_to(&udp_send, udp_addr).unwrap();

    let mut events = Vec::new();
    udp_transport
        .poll_events(Duration::from_millis(50), &mut events)
        .unwrap();
    let udp_packet = events.iter().any(|e| matches!(e, TransportEvent::Packet { opcode, payload, .. } if *opcode == OpCode::PlayerInput && payload.as_ref() == b"udp_shoot"));
    assert!(udp_packet, "UDP packet received through trait");

    udp_transport
        .broadcast(OpCode::StateDelta, b"udp_delta")
        .unwrap();
    udp_transport.flush().unwrap();

    let mut udp_recv = [0u8; 1024];
    let (len, _) = client_udp.recv_from(&mut udp_recv).unwrap();
    let mut udp_slice = udp_recv[..len].to_vec();
    let (u_op, u_payload) = decode_frame(&mut udp_slice).unwrap();
    assert_eq!(u_op, OpCode::StateDelta);
    assert_eq!(u_payload, b"udp_delta");
    println!("✓ 4. Raw UDP Transport roundtrip verified via GameTransport trait!");

    // -------------------------------------------------------------------------
    // TEST 3: Colyseus WebSocket SDK Transport via GameTransport Trait
    // -------------------------------------------------------------------------
    let ws_addr = "127.0.0.1:19430".parse().unwrap();
    let mut ws_transport = create_transport(TransportType::ColyseusWs, ws_addr)
        .expect("Create Colyseus WS transport ok");
    assert_eq!(
        ws_transport.transport_name(),
        "Colyseus WebSocket (SDK Compatible)"
    );
    println!("✓ 5. Transport created: {}", ws_transport.transport_name());

    let mut ws_client = TcpStream::connect(ws_addr).expect("WS connect ok");
    ws_client.set_nodelay(true).unwrap();
    ws_client
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();

    // Send RFC 6455 HTTP WebSocket Upgrade Request from simulated Colyseus SDK client
    let upgrade_req = "GET /game_room HTTP/1.1\r\n\
                       Host: localhost:19430\r\n\
                       Upgrade: websocket\r\n\
                       Connection: Upgrade\r\n\
                       Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
                       Sec-WebSocket-Version: 13\r\n\r\n";
    ws_client.write_all(upgrade_req.as_bytes()).unwrap();

    // Poll transport to complete handshake
    for _ in 0..5 {
        let _ = ws_transport.poll_events(Duration::from_millis(20), &mut Vec::new());
        let _ = ws_transport.flush();
        if ws_transport.ccu() > 0 {
            break;
        }
    }

    // Read 101 Switching Protocols response
    let mut hs_resp = [0u8; 1024];
    let n = ws_client.read(&mut hs_resp).unwrap();
    let resp_str = String::from_utf8_lossy(&hs_resp[..n]);
    assert!(resp_str.contains("HTTP/1.1 101 Switching Protocols"));
    assert!(resp_str.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo="));
    println!("✓ 6. Colyseus SDK WebSocket Upgrade handshake confirmed (101 Switching Protocols)!");

    // The server proactively initiates the JOIN_ROOM handshake (opcode 10)
    // right after the upgrade (SDK clients wait for it before onJoin). It may
    // arrive in the same read as the 101 headers or in a follow-up read.
    // Search the RAW bytes — the lossy string mangles 0x82.
    let join_reply: Vec<u8> = match hs_resp[..n].iter().position(|&b| b == 0x82) {
        Some(pos) => hs_resp[pos..n].to_vec(),
        None => {
            let mut reply_buf = [0u8; 1024];
            let n2 = ws_client.read(&mut reply_buf).unwrap();
            reply_buf[..n2].to_vec()
        }
    };
    assert!(
        join_reply.len() > 2,
        "expected server-initiated JOIN_ROOM frame"
    );
    assert_eq!(join_reply[0], 0x82, "Server sent WebSocket binary frame");
    assert_eq!(
        join_reply[2], 10,
        "Server sent Colyseus JOIN_ROOM opcode (10)!"
    );
    println!("✓ 7. Server-initiated JOIN_ROOM handshake verified (opcode 10, SDK format)!");

    // -------------------------------------------------------------------------
    // TEST 4: QUIC Transport via GameTransport Trait
    // -------------------------------------------------------------------------
    let quic_addr = "127.0.0.1:19440".parse().unwrap();
    let quic_transport =
        create_transport(TransportType::Quic, quic_addr).expect("Create QUIC transport ok");
    assert_eq!(quic_transport.transport_name(), "QUIC (Datagram & Streams)");
    println!(
        "✓ 8. Transport created: {}",
        quic_transport.transport_name()
    );

    println!("\n==================================================================");
    println!(" ALL 4 TRANSPORTS AUTO-SWITCHABLE & ADHERE TO GameTransport TRAIT! ");
    println!("==================================================================\n");
}
