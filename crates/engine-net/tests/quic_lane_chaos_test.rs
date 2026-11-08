use engine_net::quic::{
    QuicEngine, QuicEvent, endpoint_config, frame_bytes, self_signed_server_config,
};
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

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

fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

fn client_config() -> quinn_proto::ClientConfig {
    install_crypto_provider();
    let tls = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(TrustAnything))
        .with_no_client_auth();
    let crypto =
        quinn_proto::crypto::rustls::QuicClientConfig::try_from(tls).expect("client crypto");
    quinn_proto::ClientConfig::new(Arc::new(crypto))
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

struct LossyPipe {
    rng: Rng,

    loss_permille: u64,

    jitter_ms: u64,

    dup_permille: u64,

    queue: VecDeque<(Instant, SocketAddr, Vec<u8>)>,
    pub dropped: u64,
    pub delivered: u64,
    pub duplicated: u64,
}

impl LossyPipe {
    fn new(seed: u64, loss_permille: u64, jitter_ms: u64, dup_permille: u64) -> Self {
        Self {
            rng: Rng(seed | 1),
            loss_permille,
            jitter_ms,
            dup_permille,
            queue: VecDeque::new(),
            dropped: 0,
            delivered: 0,
            duplicated: 0,
        }
    }

    fn send(&mut self, now: Instant, from: SocketAddr, bytes: &[u8]) {
        if self.rng.below(1000) < self.loss_permille {
            self.dropped += 1;
            return;
        }
        let delay = if self.jitter_ms > 0 {
            self.rng.below(self.jitter_ms)
        } else {
            0
        };
        self.queue
            .push_back((now + Duration::from_millis(delay), from, bytes.to_vec()));
        if self.rng.below(1000) < self.dup_permille {
            self.duplicated += 1;
            let extra_delay = if self.jitter_ms > 0 {
                self.rng.below(self.jitter_ms)
            } else {
                0
            };
            self.queue.push_back((
                now + Duration::from_millis(extra_delay),
                from,
                bytes.to_vec(),
            ));
        }
    }

    fn flush_due(&mut self, now: Instant, into: &mut QuicEngine) {
        loop {
            let due = matches!(self.queue.front(), Some((at, _, _)) if *at <= now);
            if !due {
                break;
            }
            let (_, from, bytes) = self.queue.pop_front().unwrap();
            into.handle_datagram(now, from, &bytes);
            self.delivered += 1;
        }
    }
}

const SERVER_ADDR: SocketAddr =
    SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 19443);
const CLIENT_ADDR: SocketAddr =
    SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 50177);

struct Harness {
    server: QuicEngine,
    client: QuicEngine,
    to_server: LossyPipe,
    to_client: LossyPipe,
    now: Instant,
}

impl Harness {
    fn new(seed: u64, loss_permille: u64, jitter_ms: u64, dup_permille: u64) -> Self {
        install_crypto_provider();
        let server = QuicEngine::server(
            endpoint_config(),
            self_signed_server_config().expect("self-signed server config"),
        );
        let client = QuicEngine::client(endpoint_config());
        Self {
            server,
            client,
            to_server: LossyPipe::new(seed, loss_permille, jitter_ms, dup_permille),
            to_client: LossyPipe::new(seed ^ 0xBEEF, loss_permille, jitter_ms, dup_permille),
            now: Instant::now(),
        }
    }

    fn step(&mut self) -> Vec<(bool, QuicEvent)> {
        self.now += Duration::from_millis(5);
        let now = self.now;

        while let Some((dst, bytes)) = self.client.poll_outbound(now) {
            let _ = dst;
            self.to_server.send(now, CLIENT_ADDR, &bytes);
        }
        while let Some((dst, bytes)) = self.server.poll_outbound(now) {
            let _ = dst;
            self.to_client.send(now, SERVER_ADDR, &bytes);
        }

        self.to_server.flush_due(now, &mut self.server);
        self.to_client.flush_due(now, &mut self.client);

        self.server.accept_pending(now);

        let mut events = Vec::new();
        for event in self.server.poll_events() {
            events.push((true, event));
        }
        for event in self.client.poll_events() {
            events.push((false, event));
        }

        if let Some(at) = self.server.next_timeout() {
            if at <= now {
                self.server.handle_timeout(now);
            }
        }
        if let Some(at) = self.client.next_timeout() {
            if at <= now {
                self.client.handle_timeout(now);
            }
        }
        events
    }

    fn run_steps(&mut self, steps: usize) -> Vec<(bool, QuicEvent)> {
        let mut all = Vec::new();
        for _ in 0..steps {
            all.extend(self.step());
        }
        all
    }
}

#[test]
fn handshake_survives_loss_and_jitter() {
    let mut h = Harness::new(42, 150, 80, 20);
    let handle = h
        .client
        .connect(h.now, client_config(), SERVER_ADDR, "cyg.local")
        .expect("connect starts");

    let _ = handle;
    let events = h.run_steps(600);
    assert!(
        events
            .iter()
            .any(|(is_server, e)| matches!(e, QuicEvent::Connected(_)) && !is_server),
        "client handshake must complete under 15% loss + jitter"
    );
    assert!(
        events
            .iter()
            .any(|(is_server, e)| matches!(e, QuicEvent::Connected(_)) && *is_server),
        "server must accept the connection"
    );
}

#[test]
fn reliable_stream_is_byte_exact_through_the_pipe() {
    let mut h = Harness::new(137, 150, 80, 20);
    h.client
        .connect(h.now, client_config(), SERVER_ADDR, "cyg.local")
        .expect("connect starts");
    let events = h.run_steps(600);
    assert!(
        events
            .iter()
            .any(|(_, e)| matches!(e, QuicEvent::Connected(_))),
        "handshake must complete"
    );

    let payload: Vec<u8> = (0..65_536u32).map(|i| (i % 251) as u8).collect();
    let handle = h.client.first_connection().expect("connection live");
    let stream = h.client.open_bi(handle).expect("open bi stream");
    for chunk in payload.chunks(8192) {
        assert!(h.client.stream_write(handle, stream, chunk));
    }
    assert!(h.client.stream_finish(handle, stream));

    let mut received = Vec::new();
    for _ in 0..1200 {
        let events = h.step();
        for (_, event) in events {
            if let QuicEvent::StreamData { bytes, .. } = event {
                received.extend_from_slice(&bytes);
            }
        }
        if received.len() >= payload.len() {
            break;
        }
    }

    assert_eq!(received.len(), payload.len(), "every byte must arrive");

    pretty_assertions::assert_eq!(received, payload, "stream must be byte-exact");
}

#[test]
fn unreliable_datagrams_survive_verbatim_and_never_duplicate() {
    let mut h = Harness::new(7, 200, 60, 30);
    h.client
        .connect(h.now, client_config(), SERVER_ADDR, "cyg.local")
        .expect("connect starts");
    let events = h.run_steps(600);
    assert!(
        events
            .iter()
            .any(|(_, e)| matches!(e, QuicEvent::Connected(_))),
        "handshake must complete"
    );

    let sent: Vec<Vec<u8>> = (0..100u32)
        .map(|i| {
            let mut d = vec![0x04u8];
            d.extend_from_slice(&i.to_le_bytes());
            d.extend_from_slice(&[0xAA; 64]);
            d
        })
        .collect();
    let handle = h.server.first_connection().expect("server saw the client");
    for d in &sent {
        assert!(h.server.datagram_send(handle, d.clone()));
    }

    let mut received: Vec<Vec<u8>> = Vec::new();
    for _ in 0..1200 {
        let events = h.step();
        for (_, event) in events {
            if let QuicEvent::Datagram { bytes, .. } = event {
                received.push(bytes);
            }
        }
        if received.len() + (h.to_client.dropped as usize) >= sent.len() {}
    }

    assert!(!received.is_empty(), "datagrams must get through the pipe");

    for bytes in &received {
        assert!(
            sent.iter().any(|s| s == bytes),
            "a received datagram must be one we sent, verbatim"
        );
    }
    let unique: std::collections::HashSet<&Vec<u8>> = received.iter().collect();
    assert_eq!(
        unique.len(),
        received.len(),
        "QUIC datagrams must never be delivered twice"
    );

    assert!(
        received.len() < sent.len(),
        "with 20% loss across 100 datagrams, some must stay lost"
    );
    assert!(h.to_client.dropped > 0, "the pipe actually dropped packets");
}

#[test]
fn connection_close_surfaces_on_the_peer() {
    let mut h = Harness::new(99, 100, 40, 0);
    h.client
        .connect(h.now, client_config(), SERVER_ADDR, "cyg.local")
        .expect("connect starts");
    h.run_steps(600);

    let handle = h.client.first_connection().expect("connection live");
    assert!(h.client.connection_close(h.now, handle, "test-close"));

    let mut server_lost = false;
    let mut reason_seen = String::new();
    for _ in 0..400 {
        let events = h.step();
        for (_, event) in events {
            if let QuicEvent::Lost(_, reason) = event {
                if let quinn_proto::ConnectionError::ApplicationClosed(close) = reason {
                    reason_seen = String::from_utf8_lossy(&close.reason).into_owned();
                }
                server_lost = true;
            }
        }
        if server_lost {
            break;
        }
    }
    assert!(server_lost, "the peer must see the close");
    assert_eq!(
        reason_seen, "test-close",
        "the close reason must survive the wire"
    );
}

#[test]
fn frame_bytes_layout_is_len_prefixed() {
    let framed = frame_bytes(b"ping");
    assert_eq!(&framed[..4], &[0, 0, 0, 4]);
    assert_eq!(&framed[4..], b"ping");
}
