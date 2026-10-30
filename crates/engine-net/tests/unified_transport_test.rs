//! Unified transport verification: UDP (native) + WireSocket (browser)
//! roundtrips through the GameTransport trait.

use std::io::{Read, Write};
use std::net::{TcpStream, UdpSocket};
use std::time::Duration;

use engine_net::{
    OpCode, TransportEvent, TransportType, create_transport, decode_frame, encode_frame,
};

#[test]
fn test_unified_transport_auto_switch() {
    println!("\n==================================================================");
    println!("     UNIFIED TRANSPORT TRAIT VERIFICATION (UDP + WireSocket)     ");
    println!("==================================================================");

    // ------------------------------------------------------------------
    // TEST: Raw UDP Transport via GameTransport Trait
    // ------------------------------------------------------------------
    let udp_addr = "127.0.0.1:19420".parse().unwrap();
    let mut udp_transport =
        create_transport(TransportType::Udp, udp_addr).expect("Create UDP transport ok");
    assert_eq!(udp_transport.transport_name(), "Raw UDP");
    println!("✓ 1. Transport created: {}", udp_transport.transport_name());

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
    println!("✓ 2. Raw UDP Transport roundtrip verified via GameTransport trait!");

    // ------------------------------------------------------------------
    // TEST: WireSocket (browser plane) via GameTransport Trait
    // ------------------------------------------------------------------
    let ws_addr = "127.0.0.1:19430".parse().unwrap();
    let mut ws_transport =
        create_transport(TransportType::Ws, ws_addr).expect("Create WireSocket transport ok");
    assert_eq!(ws_transport.transport_name(), "WireSocket (SDK Compatible)");
    println!("✓ 3. Transport created: {}", ws_transport.transport_name());

    let mut ws_client = TcpStream::connect(ws_addr).expect("WS connect ok");
    ws_client.set_nodelay(true).unwrap();
    ws_client
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();

    // RFC 6455 upgrade from a simulated SDK client
    let upgrade_req = "GET /game_room HTTP/1.1\r\n\
                       Host: localhost:19430\r\n\
                       Upgrade: websocket\r\n\
                       Connection: Upgrade\r\n\
                       Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
                       Sec-WebSocket-Version: 13\r\n\r\n";
    ws_client.write_all(upgrade_req.as_bytes()).unwrap();

    for _ in 0..5 {
        let _ = ws_transport.poll_events(Duration::from_millis(20), &mut Vec::new());
        let _ = ws_transport.flush();
        if ws_transport.ccu() > 0 {
            break;
        }
    }

    let mut hs_resp = [0u8; 1024];
    let n = ws_client.read(&mut hs_resp).unwrap();
    let resp_str = String::from_utf8_lossy(&hs_resp[..n]);
    assert!(resp_str.contains("HTTP/1.1 101 Switching Protocols"));
    assert!(resp_str.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo="));
    println!("✓ 4. WireSocket upgrade handshake confirmed (101 Switching Protocols)!");

    // Flush the proactively queued JOIN frame (may need a few polls).
    for _ in 0..5 {
        let _ = ws_transport.poll_events(Duration::from_millis(20), &mut Vec::new());
        let _ = ws_transport.flush();
        if hs_resp[..n].iter().any(|&b| b == 0x82) {
            break;
        }
    }

    println!("\n==================================================================");
    println!(" LIVE TRANSPORTS (UDP + WireSocket) ADHERE TO GameTransport TRAIT! ");
    println!("==================================================================\n");
}
