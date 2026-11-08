use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use mio::net::UdpSocket as MioUdp;
use mio::{Interest, Poll, Token};

use super::QuicTransport;
use super::frame_bytes;
use crate::quic::{QuicEngine, endpoint_config, self_signed_server_config};

impl QuicTransport {
    pub fn new(bind: SocketAddr) -> io::Result<Self> {
        let std_sock = std::net::UdpSocket::bind(bind)?;
        std_sock.set_nonblocking(true)?;
        let mut sock = MioUdp::from_std(std_sock);
        let poll = Poll::new()?;
        poll.registry()
            .register(&mut sock, Token(0), Interest::READABLE)?;
        let engine = QuicEngine::server(
            endpoint_config(),
            self_signed_server_config().map_err(|e| io::Error::other(e.to_string()))?,
        );
        Ok(Self {
            poll,
            events: mio::Events::with_capacity(16),
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

    pub fn poll_inbound(&mut self) -> Vec<(Arc<str>, crate::transport::webrtc::contract::Inbound)> {
        std::mem::take(&mut self.inbound)
    }

    pub fn knows(&self, client_id: &str) -> bool {
        self.by_client.contains_key(client_id)
    }

    pub(super) fn mint(&mut self, handle: quinn_proto::ConnectionHandle, peer: SocketAddr) {
        let id: Arc<str> = format!("q{}", self.next_id).into();
        self.next_id += 1;
        self.by_handle.insert(handle, id.clone());
        self.by_client.insert(id.clone(), handle);
        self.remote.insert(id.clone(), peer);
        self.pending
            .push(crate::transport::TransportEvent::Connected {
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

    pub(super) fn readable(&mut self, now: Instant) {
        let mut buf = [0u8; 1500];
        for _ in 0..64 {
            match self.sock.recv_from(&mut buf) {
                Ok((n, from)) => self.engine.handle_datagram(now, from, &buf[..n]),
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
    }

    pub(super) fn flush_outbound(&mut self, now: Instant) {
        while let Some((dest, bytes)) = self.engine.poll_outbound(now) {
            let _ = self.sock.send_to(&bytes, dest);
        }
    }

    pub(super) fn send_frame(&mut self, id: &Arc<str>, frame: Vec<u8>) {
        match self.reply_stream.get(id).copied() {
            Some(stream) => {
                let handle = self.by_client[id];
                let framed = frame_bytes(&frame);
                self.engine.stream_write(handle, stream, &framed);
            }
            None => {
                self.pending_reliable
                    .entry(id.clone())
                    .or_default()
                    .push(frame);
            }
        }
    }
}
