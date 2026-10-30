//! QUIC — the fourth data lane, as a pure sans-IO state machine.
//!
//! Same shape as the str0m WebRTC plane: the engine owns no thread and no
//! socket. The caller (the host loop's mio poll, or an in-memory test) feeds
//! datagrams in with [`QuicEngine::handle_datagram`], drives [`QuicEngine::handle_timeout`],
//! and drains packets with [`QuicEngine::poll_outbound`] and application events
//! with [`QuicEngine::poll_events`]. Everything is one owner, zero locks, zero
//! async runtime.
//!
//! This is the QUIC foundation the WebTransport handshake layers onto
//! (`wtransport-proto`'s H3/session state machines ride the streams this
//! engine drives). The two app lanes map onto QUIC's native model:
//! bidirectional streams = the reliable, ordered lane; datagrams = the
//! unreliable, newest-wins lane.

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use bytes::BytesMut;
use quinn_proto::{
    ClientConfig, Connection, ConnectionError, ConnectionHandle, DatagramEvent, Dir, Endpoint,
    EndpointConfig, Event, ServerConfig, StreamEvent, Transmit,
};

/// One application-visible event off a QUIC connection.
#[derive(Debug)]
pub enum QuicEvent {
    /// Handshake completed; the connection is live.
    Connected(ConnectionHandle),
    /// The peer closed the connection, or an error ended it.
    Lost(ConnectionHandle, ConnectionError),
    /// Reliable-lane bytes read off a stream. Chunks arrive in order; the app
    /// layer above frames them (`[u32 BE][payload]` at the WebTransport tier).
    StreamData {
        handle: ConnectionHandle,
        id: quinn_proto::StreamId,
        bytes: Vec<u8>,
    },
    /// The peer finished (FIN'd) this stream.
    StreamFinished {
        handle: ConnectionHandle,
        id: quinn_proto::StreamId,
    },
    /// The peer opened a bidirectional stream and we should read it.
    StreamOpened {
        handle: ConnectionHandle,
        id: quinn_proto::StreamId,
    },
    /// One unreliable datagram, verbatim.
    Datagram {
        handle: ConnectionHandle,
        bytes: Vec<u8>,
    },
}

/// A sans-IO QUIC endpoint: server or client, exactly one owner.
pub struct QuicEngine {
    endpoint: Endpoint,
    server_config: Option<Arc<ServerConfig>>,
    connections: HashMap<ConnectionHandle, Connection>,
    pending: VecDeque<quinn_proto::Incoming>,
    outbox: VecDeque<(SocketAddr, Vec<u8>)>,
    /// Packet-coalescing scratch the endpoint API requires; kept allocated.
    buf: Vec<u8>,
}

impl QuicEngine {
    /// A server-side endpoint: accepts incoming connections with `server_config`.
    pub fn server(endpoint_config: EndpointConfig, server_config: ServerConfig) -> Self {
        Self::new(
            endpoint_config,
            Some(server_config),
            HashMap::new(),
            VecDeque::new(),
        )
    }

    /// A client-side endpoint.
    pub fn client(endpoint_config: EndpointConfig) -> Self {
        Self::new(endpoint_config, None, HashMap::new(), VecDeque::new())
    }

    fn new(
        endpoint_config: EndpointConfig,
        server_config: Option<ServerConfig>,
        connections: HashMap<ConnectionHandle, Connection>,
        pending: VecDeque<quinn_proto::Incoming>,
    ) -> Self {
        let server_config = server_config.map(Arc::new);
        Self {
            endpoint: Endpoint::new(
                Arc::new(endpoint_config),
                server_config.clone(),
                false,
                None,
            ),
            server_config,
            connections,
            pending,
            outbox: VecDeque::new(),
            buf: Vec::with_capacity(1500),
        }
    }

    /// Start connecting to a server (client endpoints only).
    pub fn connect(
        &mut self,
        now: Instant,
        config: ClientConfig,
        remote: SocketAddr,
        server_name: &str,
    ) -> Result<ConnectionHandle, quinn_proto::ConnectError> {
        let (handle, conn) = self.endpoint.connect(now, config, remote, server_name)?;
        self.connections.insert(handle, conn);
        Ok(handle)
    }

    /// Feed one UDP datagram in (str0m's `Input::Receive` equivalent).
    pub fn handle_datagram(&mut self, now: Instant, from: SocketAddr, data: &[u8]) {
        // The endpoint APPENDS into the shared scratch buffer, so it must start
        // empty on every call — reusing it without clearing grows it forever.
        self.buf.clear();
        match self
            .endpoint
            .handle(now, from, None, None, BytesMut::from(data), &mut self.buf)
        {
            Some(DatagramEvent::Response(transmit)) => self.push_transmit(transmit),
            Some(DatagramEvent::ConnectionEvent(handle, event)) => {
                if let Some(conn) = self.connections.get_mut(&handle) {
                    conn.handle_event(event);
                }
            }
            Some(DatagramEvent::NewConnection(incoming)) => {
                self.pending.push_back(incoming);
            }
            None => {}
        }
    }

    /// Accept every pending incoming connection (server endpoints).
    pub fn accept_pending(&mut self, now: Instant) -> Vec<ConnectionHandle> {
        let mut accepted = Vec::new();
        while let Some(incoming) = self.pending.pop_front() {
            match self
                .endpoint
                .accept(incoming, now, &mut self.buf, self.server_config.clone())
            {
                Ok((handle, conn)) => {
                    self.connections.insert(handle, conn);
                    accepted.push(handle);
                }
                // Retry/VersionNegotiation/BlockedByPolicy produce their own
                // outbound packets inside `accept`; nothing to queue here.
                Err(_) => continue,
            }
        }
        accepted
    }

    /// The earliest instant any connection needs a timeout drive.
    pub fn next_timeout(&mut self) -> Option<Instant> {
        self.connections
            .values_mut()
            .filter_map(|conn| conn.poll_timeout())
            .min()
    }

    /// Drive every connection whose timer has elapsed.
    pub fn handle_timeout(&mut self, now: Instant) {
        for conn in self.connections.values_mut() {
            if conn.poll_timeout().is_some_and(|at| at <= now) {
                conn.handle_timeout(now);
            }
        }
    }

    /// Drain one queued packet (`str0m`'s `Output::Transmit` equivalent).
    /// Returns `(destination, bytes)` — the caller puts it on the wire.
    pub fn poll_outbound(&mut self, now: Instant) -> Option<(SocketAddr, Vec<u8>)> {
        if let Some(pair) = self.outbox.pop_front() {
            return Some(pair);
        }
        for conn in self.connections.values_mut() {
            self.buf.clear();
            if let Some(transmit) = conn.poll_transmit(now, 1, &mut self.buf) {
                // `poll_transmit` wrote into the shared scratch buffer; this
                // copy is the one allocation on the send path.
                let pair = (transmit.destination, self.buf[..transmit.size].to_vec());
                return Some(pair);
            }
        }
        None
    }

    /// Drain application events. Also routes the connections'
    /// endpoint-facing events (CID churn, drains) back into the endpoint.
    pub fn poll_events(&mut self) -> Vec<QuicEvent> {
        let mut events = Vec::new();
        let handles: Vec<ConnectionHandle> = self.connections.keys().copied().collect();
        for handle in handles {
            let Some(conn) = self.connections.get_mut(&handle) else {
                continue;
            };
            loop {
                let Some(endpoint_event) = conn.poll_endpoint_events() else {
                    break;
                };
                if let Some(event) = self.endpoint.handle_event(handle, endpoint_event) {
                    conn.handle_event(event);
                }
            }
            loop {
                let Some(event) = conn.poll() else {
                    break;
                };
                match event {
                    Event::Connected => events.push(QuicEvent::Connected(handle)),
                    Event::ConnectionLost { reason } => {
                        events.push(QuicEvent::Lost(handle, reason));
                    }
                    Event::Stream(StreamEvent::Opened { dir }) => {
                        if dir == Dir::Bi {
                            // The opening STREAM frame may already carry data —
                            // `Readable` only fires for arrivals AFTER the
                            // stream was reported, so drain on accept too.
                            let mut opened: Vec<quinn_proto::StreamId> = Vec::new();
                            while let Some(id) = conn.streams().accept(Dir::Bi) {
                                opened.push(id);
                            }
                            for id in opened {
                                events.push(QuicEvent::StreamOpened { handle, id });
                                read_stream(conn, handle, id, &mut events);
                            }
                        }
                    }
                    Event::Stream(StreamEvent::Readable { id }) => {
                        read_stream(conn, handle, id, &mut events);
                    }
                    Event::Stream(StreamEvent::Finished { id }) => {
                        events.push(QuicEvent::StreamFinished { handle, id });
                    }
                    Event::DatagramReceived => {
                        while let Some(bytes) = conn.datagrams().recv() {
                            events.push(QuicEvent::Datagram {
                                handle,
                                bytes: bytes.to_vec(),
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
        events
    }

    /// Any live connection's handle (tests and single-connection callers).
    pub fn first_connection(&self) -> Option<ConnectionHandle> {
        self.connections.keys().next().copied()
    }

    /// Open one bidirectional stream (client-initiated requests).
    pub fn open_bi(&mut self, handle: ConnectionHandle) -> Option<quinn_proto::StreamId> {
        self.connections
            .get_mut(&handle)
            .and_then(|conn| conn.streams().open(Dir::Bi))
    }

    /// Reliable lane: write on a stream (client-opened or accepted).
    pub fn stream_write(
        &mut self,
        handle: ConnectionHandle,
        id: quinn_proto::StreamId,
        bytes: &[u8],
    ) -> bool {
        self.connections
            .get_mut(&handle)
            .map(|conn| conn.send_stream(id).write(bytes).is_ok())
            .unwrap_or(false)
    }

    /// Reliable lane: FIN a stream after its last write.
    pub fn stream_finish(&mut self, handle: ConnectionHandle, id: quinn_proto::StreamId) -> bool {
        self.connections
            .get_mut(&handle)
            .map(|conn| conn.send_stream(id).finish().is_ok())
            .unwrap_or(false)
    }

    /// Server-initiated close: a CONNECTION_CLOSE frame goes out on the next
    /// transmit pass, the peer sees a clean goodbye.
    pub fn connection_close(&mut self, handle: ConnectionHandle, reason: &str) -> bool {
        let now = Instant::now();
        self.connections
            .get_mut(&handle)
            .map(|conn| {
                conn.close(
                    now,
                    0u32.into(),
                    bytes::Bytes::copy_from_slice(reason.as_bytes()),
                );
                true
            })
            .unwrap_or(false)
    }

    /// The peer's socket address, once the connection exists.
    pub fn remote_addr(&self, handle: ConnectionHandle) -> Option<SocketAddr> {
        self.connections
            .get(&handle)
            .map(|conn| conn.remote_address())
    }

    /// Unreliable lane: one datagram toward the peer.
    pub fn datagram_send(&mut self, handle: ConnectionHandle, bytes: Vec<u8>) -> bool {
        self.connections
            .get_mut(&handle)
            .map(|conn| conn.datagrams().send(bytes.into(), true).is_ok())
            .unwrap_or(false)
    }

    fn push_transmit(&mut self, transmit: Transmit) {
        self.outbox
            .push_back((transmit.destination, self.buf[..transmit.size].to_vec()));
    }
}

/// Drain whatever is readable on one stream into `StreamData` events.
/// `Chunks` borrows the stream, so read + copy + finalize share a scope and
/// the copied bytes travel out in the event.
fn read_stream(
    conn: &mut Connection,
    handle: ConnectionHandle,
    id: quinn_proto::StreamId,
    events: &mut Vec<QuicEvent>,
) {
    let mut recv = conn.recv_stream(id);
    let mut chunks = match recv.read(true) {
        Ok(chunks) => chunks,
        Err(_) => return,
    };
    loop {
        let chunk = match chunks.next(64 * 1024) {
            Ok(Some(chunk)) => chunk,
            _ => break,
        };
        let bytes = chunk.bytes.to_vec();
        events.push(QuicEvent::StreamData { handle, id, bytes });
    }
    // `finalize` consumes the reader — it must come after the drain loop.
    let _ = chunks.finalize();
}

/// `[u32 BE][payload]` — the reliable-lane frame, shared by the engine and
/// the transport's framing (one definition so the two never drift).
pub fn frame_bytes(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + payload.len());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

/// The endpoint reset key QUIC stateless resets need, from ring's HMAC.
pub fn endpoint_config() -> EndpointConfig {
    let rng = ring::rand::SystemRandom::new();
    let key =
        ring::hmac::Key::generate(ring::hmac::HMAC_SHA256, &rng).expect("ring hmac key generation");
    EndpointConfig::new(Arc::new(key))
}

/// A self-signed certificate chain + key pair as `ServerConfig` wants them
/// (`rcgen`-backed; browsers pin its SHA-256 via WebTransport's
/// `serverCertificateHashes`).
pub fn self_signed_server_config() -> Result<ServerConfig, Box<dyn std::error::Error>> {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
    let pair = rcgen::generate_simple_self_signed(vec!["cyg.local".to_owned()])?;
    let cert_der: CertificateDer<'static> = pair.cert.der().to_owned();
    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(pair.signing_key.serialize_der()));
    Ok(ServerConfig::with_single_cert(vec![cert_der], key_der)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const SERVER_ADDR: SocketAddr =
        SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 19443);
    const CLIENT_ADDR: SocketAddr =
        SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 50123);

    /// A rustls verifier that trusts the self-signed test certificate — the
    /// production analogue is WebTransport's `serverCertificateHashes` pin.
    #[derive(Debug)]
    struct TrustAnything;

    impl rustls::client::danger::ServerCertVerifier for TrustAnything {
        fn verify_server_cert(
            &self,
            _end_entity: &rustls::pki_types::CertificateDer<'_>,
            _intermediates: &[rustls::pki_types::CertificateDer<'_>],
            _server_name: &rustls::pki_types::ServerName<'_>,
            _ocsp_response: &[u8],
            _now: rustls::pki_types::UnixTime,
        ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            _message: &[u8],
            _cert: &rustls::pki_types::CertificateDer<'_>,
            _dss: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
        }

        fn verify_tls13_signature(
            &self,
            _message: &[u8],
            _cert: &rustls::pki_types::CertificateDer<'_>,
            _dss: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
        }

        fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
            use rustls::SignatureScheme::*;
            vec![
                RSA_PSS_SHA256,
                ECDSA_NISTP256_SHA256,
                ED25519,
                RSA_PKCS1_SHA256,
            ]
        }
    }

    fn client_config() -> ClientConfig {
        let tls = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(TrustAnything))
            .with_no_client_auth();
        let crypto =
            quinn_proto::crypto::rustls::QuicClientConfig::try_from(tls).expect("client crypto");
        ClientConfig::new(Arc::new(crypto))
    }

    /// Push every packet each engine queues into the other, drive timeouts,
    /// and advance the fake clock whenever the wire goes idle. No sockets —
    /// the two engines hand transmits straight to each other.
    fn wire(server: &mut QuicEngine, client: &mut QuicEngine, now: &mut Instant) {
        for _ in 0..400 {
            let mut moved = false;

            // The server must pull its pending `Incoming`s — the accept call
            // itself produces the handshake's server-side flight.
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

        // Reliable lane: the client opens a bidirectional stream and sends one
        // length-prefixed app frame; the server reads it and echoes on the
        // SAME stream — the WebTransport session shape.
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

        // Unreliable lane: datagrams both ways.
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
}
