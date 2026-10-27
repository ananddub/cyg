//! QUIC/WebTransport as a `GameTransport`: the mio-socket wrapper around the
//! sans-IO [`QuicEngine`], the same shape as the WebRTC transport — blocking
//! `poll_events` on the socket, lane-classified `poll_inbound` for the loop,
//! sends mapped reliability-first.
//!
//! The wire contract is the WebRTC contract verbatim (that is the point):
//! unreliable-lane datagrams carry the input frames, one client-opened
//! bidirectional stream carries the reliable lane as
//! `[u32 BE len][payload]` app frames, and [`classify_inbound`] decodes both
//! lanes. A client id looks like `q7`; nothing else in the engine may mint
//! that namespace.

use std::collections::HashMap;
use std::sync::Arc;
use std::io;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use bytes::BytesMut;
use mio::net::UdpSocket as MioUdp;
use mio::{Events, Interest, Poll, Token};

use super::GameTransport;
use super::webrtc::contract::classify_inbound;
use super::webrtc::contract::{Inbound, Lane};
use crate::framing::OpCode;
use crate::quic::{endpoint_config, self_signed_server_config, QuicEngine, QuicEvent};

/// The reliable-lane frame overhead: `[u32 BE len]`.
const FRAME_HEADER: usize = 4;

pub struct QuicTransport {
    poll: Poll,
    events: Events,
    sock: MioUdp,
    engine: QuicEngine,
    /// handle -> client id (`q<n>`); minted at handshake completion.
    by_handle: HashMap<quinn_proto::ConnectionHandle, Arc<str>>,
    by_client: HashMap<Arc<str>, quinn_proto::ConnectionHandle>,
    remote: HashMap<Arc<str>, SocketAddr>,
    /// The client-opened bidirectional stream is the server's reliable reply
    /// path (the WebTransport session shape).
    reply_stream: HashMap<Arc<str>, quinn_proto::StreamId>,
    /// Reliable frames queued before the client's stream arrived.
    pending_reliable: HashMap<Arc<str>, Vec<Vec<u8>>>,
    /// Per-client stream reassembly: stream chunks split arbitrarily, the app
    /// speaks whole `[u32][payload]` frames.
    reassembly: HashMap<Arc<str>, Vec<u8>>,
    inbound: Vec<(Arc<str>, Inbound)>,
    pending: Vec<super::TransportEvent>,
    next_id: u64,
}

impl QuicTransport {
    /// Bind the QUIC listener. The self-signed certificate is generated per
    /// transport; WebTransport clients pin its SHA-256 via
    /// `serverCertificateHashes`, so no CA is involved.
    pub fn new(bind: SocketAddr) -> io::Result<Self> {
        let std_sock = std::net::UdpSocket::bind(bind)?;
        std_sock.set_nonblocking(true)?;
        let mut sock = MioUdp::from_std(std_sock);
        let poll = Poll::new()?;
        poll.registry().register(&mut sock, Token(0), Interest::READABLE)?;
        let engine = QuicEngine::server(
            endpoint_config(),
            self_signed_server_config().map_err(|e| io::Error::other(e.to_string()))?,
        );
        Ok(Self {
            poll,
            events: Events::with_capacity(16),
            sock,
            engine,
            by_handle: HashMap::new(),
            by_client: HashMap::new(),
            remote: HashMap::new(),
            reply_stream: HashMap::new(),
            pending_reliable: HashMap::new(),
            reassembly: HashMap::new(),
            inbound: Vec::new(),
            pending: Vec::new(),
            next_id: 1,
        })
    }

    /// Lane-classified inbound frames, drained once per loop iteration — the
    /// exact shape [`super::webrtc::WebRtcTransport::poll_inbound`] returns.
    pub fn poll_inbound(&mut self) -> Vec<(Arc<str>, Inbound)> {
        std::mem::take(&mut self.inbound)
    }

    /// Whether this client id belongs to the QUIC plane (send routing).
    pub fn knows(&self, client_id: &str) -> bool {
        self.by_client.contains_key(client_id)
    }

    fn mint(&mut self, handle: quinn_proto::ConnectionHandle, peer: SocketAddr) {
        let id: Arc<str> = format!("q{}", self.next_id).into();
        self.next_id += 1;
        self.by_handle.insert(handle, id.clone());
        self.by_client.insert(id.clone(), handle);
        self.remote.insert(id.clone(), peer);
        self.pending.push(super::TransportEvent::Connected {
            client_id: id,
            peer_addr: peer,
        });
    }

    fn drop_client(&mut self, id: &Arc<str>) {
        if let Some(handle) = self.by_client.remove(id) {
            self.by_handle.remove(&handle);
        }
        self.remote.remove(id);
        self.reply_stream.remove(id);
        self.pending_reliable.remove(id);
        self.reassembly.remove(id);
    }

    fn readable(&mut self, now: Instant) {
        let mut buf = [0u8; 1500];
        for _ in 0..64 {
            match self.sock.recv_from(&mut buf) {
                Ok((n, from)) => self.engine.handle_datagram(now, from, &buf[..n]),
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
    }

    fn flush_outbound(&mut self, now: Instant) {
        while let Some((dest, bytes)) = self.engine.poll_outbound(now) {
            let _ = self.sock.send_to(&bytes, dest);
        }
    }

    /// The reliable lane: one length-prefixed frame toward the client, on the
    /// stream it opened (queued until then).
    fn send_frame(&mut self, id: &Arc<str>, frame: Vec<u8>) {
        match self.reply_stream.get(id).copied() {
            Some(stream) => {
                let handle = self.by_client[id];
                let framed = frame_bytes(&frame);
                self.engine.stream_write(handle, stream, &framed);
            }
            None => {
                self.pending_reliable.entry(id.clone()).or_default().push(frame);
            }
        }
    }

    fn handle_engine_event(&mut self, event: QuicEvent) {
        match event {
            QuicEvent::Connected(handle) => {
                let peer = self.engine.remote_addr(handle).unwrap_or_else(|| {
                    std::net::SocketAddr::new(std::net::IpAddr::V4(
                        std::net::Ipv4Addr::UNSPECIFIED,
                    ), 0)
                });
                self.mint(handle, peer);
            }
            QuicEvent::StreamOpened { handle, id } => {
                let Some(cid) = self.by_handle.get(&handle).cloned() else {
                    return;
                };
                self.reply_stream.insert(cid.clone(), id);
                // Flush anything the engine queued before the stream existed.
                if let Some(queued) = self.pending_reliable.remove(&cid) {
                    for frame in queued {
                        let framed = frame_bytes(&frame);
                        self.engine.stream_write(handle, id, &framed);
                    }
                }
            }
            QuicEvent::StreamData { handle, id: _, bytes } => {
                let Some(cid) = self.by_handle.get(&handle).cloned() else {
                    return;
                };
                let buffer = self.reassembly.entry(cid.clone()).or_default();
                buffer.extend_from_slice(&bytes);
                loop {
                    if buffer.len() < FRAME_HEADER {
                        break;
                    }
                    let len = u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]])
                        as usize;
                    if buffer.len() < FRAME_HEADER + len {
                        break;
                    }
                    let frame: Vec<u8> = buffer[FRAME_HEADER..FRAME_HEADER + len].to_vec();
                    buffer.drain(..FRAME_HEADER + len);
                    let inbound = classify_inbound(Some(Lane::Reliable), &frame);
                    self.inbound.push((cid.clone(), inbound));
                }
                if buffer.is_empty() {
                    self.reassembly.remove(&cid);
                }
            }
            QuicEvent::StreamFinished { handle, id: _ } => {
                if let Some(cid) = self.by_handle.get(&handle).cloned() {
                    self.reply_stream.remove(&cid);
                }
            }
            QuicEvent::Datagram { handle, bytes } => {
                let Some(cid) = self.by_handle.get(&handle).cloned() else {
                    return;
                };
                let inbound = classify_inbound(Some(Lane::Unreliable), &bytes);
                self.inbound.push((cid, inbound));
            }
            QuicEvent::Lost(handle, _reason) => {
                if let Some(cid) = self.by_handle.remove(&handle) {
                    self.by_client.remove(&cid);
                    self.remote.remove(&cid);
                    self.reply_stream.remove(&cid);
                    self.pending_reliable.remove(&cid);
                    self.reassembly.remove(&cid);
                    self.pending.push(super::TransportEvent::Disconnected { client_id: cid });
                }
            }
        }
    }
}

impl GameTransport for QuicTransport {
    fn poll_events(&mut self, timeout: Duration, events: &mut Vec<super::TransportEvent>) -> io::Result<()> {
        events.append(&mut self.pending);
        let now = Instant::now();
        // Sleep until a datagram lands or the QUIC timers come due — whichever
        // first, so retransmits and handshake deadlines never slip far.
        let wait = self
            .engine
            .next_timeout()
            .map(|at| at.saturating_duration_since(now))
            .map_or(timeout, |left| left.min(timeout));
        self.events.clear();
        self.poll.poll(&mut self.events, Some(wait))?;
        if self.events.is_empty() {
            // Idle wake: still drive QUIC timers below.
        }
        let now = Instant::now();
        if self.events.iter().any(|e| e.token() == Token(0)) {
            self.readable(now);
        }
        // Pending `Incoming`s must be pulled: the accept call itself produces
        // the server's handshake flight.
        self.engine.accept_pending(now);
        if let Some(at) = self.engine.next_timeout() {
            if at <= now {
                self.engine.handle_timeout(now);
            }
        }
        self.flush_outbound(now);

        for event in self.engine.poll_events() {
            self.handle_engine_event(event);
        }
        Ok(())
    }

    fn send_to(&mut self, client_id: &str, opcode: OpCode, payload: &[u8]) -> io::Result<()> {
        let Some(&handle) = self.by_client.get(client_id) else {
            return Ok(());
        };
        match opcode {
            // Newest-wins state: the unreliable lane, one datagram, the opcode
            // byte first so the client can classify without framing.
            OpCode::StateDelta | OpCode::StateSnapshot => {
                let mut datagram = Vec::with_capacity(1 + payload.len());
                datagram.push(opcode as u8);
                datagram.extend_from_slice(payload);
                self.engine.datagram_send(handle, datagram);
            }
            // Everything else is the reliable lane.
            _ => {
                let mut frame = Vec::with_capacity(1 + payload.len());
                frame.push(opcode as u8);
                frame.extend_from_slice(payload);
                self.send_frame(&Arc::<str>::from(client_id), frame);
            }
        }
        Ok(())
    }

    fn broadcast(&mut self, opcode: OpCode, payload: &[u8]) -> io::Result<()> {
        let ids: Vec<Arc<str>> = self.by_client.keys().cloned().collect();
        for id in ids {
            self.send_to(&id, opcode, payload)?;
        }
        Ok(())
    }

    fn disconnect(&mut self, client_id: &str) -> io::Result<()> {
        if let Some(&handle) = self.by_client.get(client_id) {
            self.engine.connection_close(handle, "server leave");
        }
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flush_outbound(Instant::now());
        Ok(())
    }

    fn ccu(&self) -> usize {
        self.by_client.len()
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.sock.local_addr()
    }

    fn transport_name(&self) -> &'static str {
        "quic"
    }
}

/// `[u32 BE][payload]` — the reliable-lane frame, matching the engine side.
fn frame_bytes(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(FRAME_HEADER + payload.len());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quic::{QuicEngine, QuicEvent, endpoint_config, frame_bytes};
    use std::sync::Arc;
    use std::time::Duration;

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
            vec![RSA_PSS_SHA256, ECDSA_NISTP256_SHA256, ED25519, RSA_PKCS1_SHA256]
        }
    }

    fn client_config() -> quinn_proto::ClientConfig {
        let tls = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(TrustAnything))
            .with_no_client_auth();
        let crypto = quinn_proto::crypto::rustls::QuicClientConfig::try_from(tls)
            .expect("client crypto");
        quinn_proto::ClientConfig::new(Arc::new(crypto))
    }

    #[test]
    fn quic_transport_serves_a_real_socket_client() {
        let mut transport = QuicTransport::new("127.0.0.1:0".parse().unwrap()).expect("bind");
        let server_addr = transport.local_addr().unwrap();

        // The client is the same sans-IO engine, wired to a real UDP socket.
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

            // client → transport over the real socket
            while let Some((_, bytes)) = client.poll_outbound(now) {
                sock.send(&bytes).unwrap();
            }
            transport.poll_events(Duration::from_millis(2), &mut Vec::new()).unwrap();

            // transport → client
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
            if tevents.iter().any(|e| matches!(e, crate::transport::TransportEvent::Connected { .. })) {
                transport_saw_client = true;
            }
            if client_connected && transport_saw_client {
                break;
            }
        }
        assert!(client_connected, "client handshake completed");
        assert!(transport_saw_client, "the transport reported the client");

        // Reliable lane, client → server: an SDK-shaped app frame
        // ([0x05][type_len][type][payload]) rides the client's stream with the
        // transport-level [u32] framing on top.
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
            transport.poll_events(Duration::from_millis(2), &mut Vec::new()).unwrap();
        }
        let inbound = transport.poll_inbound();
        let app_seen = inbound.iter().any(|(id, msg)| {
            matches!(msg, crate::transport::webrtc::contract::Inbound::App { data }
                if data == &app && id.starts_with('q'))
        });
        assert!(app_seen, "the app frame arrived lane-classified: {inbound:?}");

        // Unreliable lane, server → client: a StateDelta datagram keeps the
        // opcode byte first; reliable lane carries the framed opcode form.
        let cid = transport.by_client.keys().next().unwrap().clone();
        transport
            .send_to(&cid, crate::framing::OpCode::StateDelta, b"delta-bytes")
            .unwrap();
        transport.send_to(&cid, crate::framing::OpCode::Custom, b"event-bytes").unwrap();
        transport.flush().unwrap();
        let mut got_delta = false;
        let mut got_event = false;
        for _ in 0..60 {
            now += Duration::from_millis(2);
            transport.poll_events(Duration::from_millis(2), &mut Vec::new()).unwrap();
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
                        // [u32 len][opcode][payload]
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
}
