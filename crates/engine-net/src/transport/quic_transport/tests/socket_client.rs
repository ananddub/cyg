use std::time::{Duration, Instant};

use super::support::client_config;
use crate::quic::{QuicEngine, QuicEvent, endpoint_config, frame_bytes};
use crate::transport::GameTransport;
use crate::transport::quic_transport::QuicTransport;

#[test]
fn quic_transport_serves_a_real_socket_client() {
    let mut transport = QuicTransport::new("127.0.0.1:0".parse().unwrap()).expect("bind");
    let server_addr = transport.local_addr().unwrap();

    let sock = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    sock.set_nonblocking(true).unwrap();
    sock.connect(server_addr).unwrap();
    let mut client = QuicEngine::client(endpoint_config());
    let handle = client
        .connect(Instant::now(), client_config(), server_addr, "cyg.local")
        .expect("connect");

    let mut now = Instant::now();
    let mut client_connected = false;
    let mut transport_saw_client = false;
    for _ in 0..400 {
        now += Duration::from_millis(3);

        while let Some((_, bytes)) = client.poll_outbound(now) {
            sock.send(&bytes).unwrap();
        }
        transport
            .poll_events(Duration::from_millis(2), &mut Vec::new())
            .unwrap();

        let mut buf = [0u8; 1500];
        loop {
            match sock.recv(&mut buf) {
                Ok(n) => client.handle_datagram(now, server_addr, &buf[..n]),
                Err(_) => break,
            }
        }
        if let Some(at) = client.next_timeout() {
            if at <= now {
                client.handle_timeout(now);
            }
        }

        for event in client.poll_events() {
            if let QuicEvent::Connected(_) = event {
                client_connected = true;
            }
        }
        let mut tevents = Vec::new();
        transport.poll_events(Duration::ZERO, &mut tevents).unwrap();
        if tevents
            .iter()
            .any(|e| matches!(e, crate::transport::TransportEvent::Connected { .. }))
        {
            transport_saw_client = true;
        }
        if client_connected && transport_saw_client {
            break;
        }
    }
    assert!(client_connected, "client handshake completed");
    assert!(transport_saw_client, "the transport reported the client");

    let handle = client.first_connection().expect("client connection");
    let stream = client.open_bi(handle).expect("stream opened");
    let app = {
        let mut m = vec![0x05u8, 4];
        m.extend_from_slice(b"chat");
        m.extend_from_slice(b"{\"x\":1}");
        m
    };
    let framed = frame_bytes(&app);
    client.stream_write(handle, stream, &framed);
    for _ in 0..40 {
        now += Duration::from_millis(2);
        while let Some((_, bytes)) = client.poll_outbound(now) {
            sock.send(&bytes).unwrap();
        }
        transport
            .poll_events(Duration::from_millis(2), &mut Vec::new())
            .unwrap();
    }
    let inbound = transport.poll_inbound();
    let app_seen = inbound.iter().any(|(id, msg)| {
        matches!(msg, crate::transport::webrtc::contract::Inbound::App { data }
            if data == &app && id.starts_with('q'))
    });
    assert!(
        app_seen,
        "the app frame arrived lane-classified: {inbound:?}"
    );

    let cid = transport.by_client.keys().next().unwrap().clone();
    transport
        .send_to(&cid, crate::framing::OpCode::StateDelta, b"delta-bytes")
        .unwrap();
    transport
        .send_to(&cid, crate::framing::OpCode::Custom, b"event-bytes")
        .unwrap();
    transport.flush().unwrap();
    let mut got_delta = false;
    let mut got_event = false;
    for _ in 0..60 {
        now += Duration::from_millis(2);
        transport
            .poll_events(Duration::from_millis(2), &mut Vec::new())
            .unwrap();
        let mut buf = [0u8; 1500];
        loop {
            match sock.recv(&mut buf) {
                Ok(n) => client.handle_datagram(now, server_addr, &buf[..n]),
                Err(_) => break,
            }
        }
        for event in client.poll_events() {
            match event {
                QuicEvent::Datagram { bytes, .. } => {
                    if bytes == {
                        let mut d = vec![crate::framing::OpCode::StateDelta as u8];
                        d.extend_from_slice(b"delta-bytes");
                        d
                    } {
                        got_delta = true;
                    }
                }
                QuicEvent::StreamData { bytes, .. } => {
                    if bytes.len() > 5 && bytes[4] == crate::framing::OpCode::Custom as u8 {
                        got_event = true;
                    }
                }
                _ => {}
            }
        }
        if got_delta && got_event {
            break;
        }
    }
    assert!(got_delta, "the delta datagram reached the client");
    assert!(got_event, "the reliable event frame reached the client");
}
