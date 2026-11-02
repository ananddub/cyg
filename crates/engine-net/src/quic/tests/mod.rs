mod support;

use super::*;
use std::time::{Duration, Instant};

use quinn_proto::Dir;
use support::{client_config, install_crypto_provider, CLIENT_ADDR, SERVER_ADDR};

fn wire(server: &mut QuicEngine, client: &mut QuicEngine, now: &mut Instant) {
    for _ in 0..400 {
        let mut moved = false;

        if !server.accept_pending(*now).is_empty() {
            moved = true;
        }

        let mut packets = Vec::new();
        while let Some((dest, bytes)) = client.poll_outbound(*now) {
            packets.push(bytes);
            let _ = dest;
        }
        for bytes in packets {
            server.handle_datagram(*now, CLIENT_ADDR, &bytes);
            moved = true;
        }

        let mut packets = Vec::new();
        while let Some((dest, bytes)) = server.poll_outbound(*now) {
            packets.push(bytes);
            let _ = dest;
        }
        for bytes in packets {
            client.handle_datagram(*now, SERVER_ADDR, &bytes);
            moved = true;
        }

        for engine in [&mut *server, &mut *client] {
            if let Some(at) = engine.next_timeout() {
                if at <= *now {
                    engine.handle_timeout(*now);
                    moved = true;
                }
            }
        }

        if !moved {
            *now += Duration::from_millis(2);
        }
    }
}

fn drain(engine: &mut QuicEngine) -> Vec<QuicEvent> {
    engine.poll_events()
}

#[test]
fn handshake_stream_echo_and_datagram_roundtrip_in_memory() {
    install_crypto_provider();
    let mut now = Instant::now();
    let mut server = QuicEngine::server(
        endpoint_config(),
        self_signed_server_config().expect("cert"),
    );
    let mut client = QuicEngine::client(endpoint_config());

    let handle = client
        .connect(now, client_config(), SERVER_ADDR, "cyg.local")
        .expect("connect");
    wire(&mut server, &mut client, &mut now);

    let connected_client = drain(&mut client)
        .into_iter()
        .any(|e| matches!(e, QuicEvent::Connected(_)));
    let connected_server = drain(&mut server)
        .into_iter()
        .any(|e| matches!(e, QuicEvent::Connected(_)));
    assert!(connected_client, "client handshake completed");
    assert!(connected_server, "server handshake completed");

    let stream = client
        .connections
        .get_mut(&handle)
        .map(|conn| conn.streams().open(Dir::Bi))
        .unwrap_or_else(|| panic!("client connection alive"))
        .expect("stream opened");
    assert!(
        client.stream_write(handle, stream, b"ping-frame"),
        "client wrote the frame"
    );
    wire(&mut server, &mut client, &mut now);

    let mut server_saw = None;
    for event in drain(&mut server) {
        match event {
            QuicEvent::StreamData { id, bytes, .. } => {
                server_saw = Some((id, bytes));
            }
            _ => {}
        }
    }
    let (id, bytes) = server_saw.expect("server read the client frame");
    assert_eq!(bytes, b"ping-frame", "frame arrived intact");

    let server_handle = server.connections.keys().next().copied().unwrap();
    assert!(
        server.stream_write(server_handle, id, b"pong-frame"),
        "server echoed"
    );
    wire(&mut server, &mut client, &mut now);

    let echoed = drain(&mut client).into_iter().any(|e| match e {
        QuicEvent::StreamData { bytes, .. } => bytes == b"pong-frame",
        _ => false,
    });
    assert!(echoed, "the echo arrived back on the reliable lane");

    assert!(client.datagram_send(handle, b"dgram".to_vec()));
    wire(&mut server, &mut client, &mut now);
    let server_got = drain(&mut server).into_iter().any(|e| match e {
        QuicEvent::Datagram { bytes, .. } => bytes == b"dgram",
        _ => false,
    });
    assert!(server_got, "datagram reached the server");

    assert!(server.datagram_send(
        server.connections.keys().next().copied().unwrap(),
        b"back".to_vec()
    ));
    wire(&mut server, &mut client, &mut now);
    let client_got = drain(&mut client).into_iter().any(|e| match e {
        QuicEvent::Datagram { bytes, .. } => bytes == b"back",
        _ => false,
    });
    assert!(client_got, "datagram reached the client");
}
