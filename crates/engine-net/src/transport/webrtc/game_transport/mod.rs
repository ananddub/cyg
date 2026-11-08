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
        let _ = self.server.send_delta(client_id, 0, 0, payload);
        self.flush()
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
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
    }
}
