use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use mio::net::UdpSocket;

use super::server::WebRtcServer;
use crate::transport::{GameTransport, TransportEvent};

mod driver;
mod transport_impl;

pub struct WebRtcTransport {
    pub(super) server: WebRtcServer,
    pub(super) sock: UdpSocket,
    pub(super) local: SocketAddr,

    pub(super) poll: mio::Poll,
    pub(super) events: mio::Events,

    pub(super) last_seen: HashMap<Arc<str>, Instant>,
    /// Next delta seq per session, so game-lane deltas carry real increasing
    /// sequence numbers like the rtc_plane path instead of a constant 0.
    pub(super) delta_seqs: HashMap<String, u16>,
    pub(super) idle_after: Duration,
    pub(super) pending: Vec<TransportEvent>,
    pub(super) inbound: Vec<(Arc<str>, super::contract::Inbound)>,

    pub(super) schema_descriptor: Bytes,
}

impl WebRtcTransport {
    pub fn new(bind_addr: SocketAddr) -> io::Result<Self> {
        let sock = UdpSocket::bind(bind_addr)?;
        Self::from_socket(sock.into(), vec![])
    }

    pub fn from_socket(
        sock: std::net::UdpSocket,
        announced_addrs: Vec<SocketAddr>,
    ) -> io::Result<Self> {
        sock.set_nonblocking(true)?;
        let local = sock.local_addr()?;
        let mut sock = UdpSocket::from_std(sock);
        let poll = mio::Poll::new()?;
        poll.registry()
            .register(&mut sock, mio::Token(0), mio::Interest::READABLE)?;
        Ok(Self {
            server: WebRtcServer::new(announced_addrs),
            sock,
            poll,
            local,
            events: mio::Events::with_capacity(256),
            last_seen: HashMap::new(),
            delta_seqs: HashMap::new(),
            idle_after: Duration::from_secs(5),
            pending: Vec::new(),
            inbound: Vec::new(),
            schema_descriptor: Bytes::new(),
        })
    }

    pub fn server(&self) -> &WebRtcServer {
        &self.server
    }

    pub fn server_mut(&mut self) -> &mut WebRtcServer {
        &mut self.server
    }

    pub fn poll_inbound(&mut self) -> Vec<(Arc<str>, super::contract::Inbound)> {
        std::mem::take(&mut self.inbound)
    }

    pub fn set_schema_descriptor(&mut self, descriptor: impl Into<Bytes>) {
        self.schema_descriptor = descriptor.into();
    }

    pub fn send_state(&mut self, client_id: &str, payload: &[u8]) -> io::Result<()> {
        let seq = self.next_delta_seq(client_id);
        let _ = self.server.send_delta(client_id, seq, 0, payload);
        self.flush()
    }

    /// Per-session monotonic delta sequence number (wrapping at u16).
    pub(super) fn next_delta_seq(&mut self, client_id: &str) -> u16 {
        let entry = self.delta_seqs.entry(client_id.to_string()).or_insert(0);
        let seq = *entry;
        *entry = entry.wrapping_add(1);
        seq
    }

    pub(super) fn forget_delta_seq(&mut self, client_id: &str) {
        self.delta_seqs.remove(client_id);
    }

    pub fn dropped_frames(&self) -> u64 {
        self.server.dropped_frames()
    }

    fn recv_udp(&mut self) {
        let mut buf = [0u8; 4096];
        loop {
            match self.sock.recv_from(&mut buf) {
                Ok((amt, peer)) => {
                    let local = self.local;
                    self.server.handle_udp(peer, local, &buf[..amt]);
                    // Inbound UDP keeps ICE sessions alive: refresh last_seen
                    // so sweep_stale does not cut sessions that are active on
                    // the wire but idle on the data channels.
                    if let Some(cid) = self.server.addr_to_session.get(&peer) {
                        if let Some(seen) = self.last_seen.get_mut(cid.as_str()) {
                            *seen = Instant::now();
                        }
                    }
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
    }
}
