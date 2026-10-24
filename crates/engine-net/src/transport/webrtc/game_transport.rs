//! # WebRTC GameTransport adapter
//!
//! Wraps [`WebRtcServer`] + its UDP socket behind the [`GameTransport`] trait —
//! the same surface the old WS fallback exposed, so the host loop is
//! transport-agnostic. This is the R2a work item: once the host loop runs on
//! this, `transport/ws/` is deleted and the data plane is WebRTC + UDP only.

use std::io;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use bytes::Bytes;
use mio::net::UdpSocket;

use super::contract::Lane;
use super::{WebRtcServer, WebRtcServerEvent};
use crate::framing::OpCode;
use crate::transport::{GameTransport, TransportEvent};

/// GameTransport over one UDP socket driving a [`WebRtcServer`].
pub struct WebRtcTransport {
    server: WebRtcServer,
    sock: UdpSocket,
    local: SocketAddr,
    events: mio::Events,
    /// session id -> last inbound instant (idle-session sweep).
    last_seen: std::collections::HashMap<String, Instant>,
    idle_after: Duration,
    pending: Vec<TransportEvent>,
    /// `openoxide.schema.bin` bytes queued on a session's reliable lane as
    /// soon as that lane opens — the DataChannel equivalent of the WS
    /// transport sending the descriptor right after the upgrade.
    schema_descriptor: Bytes,
}

impl WebRtcTransport {
    pub fn new(bind_addr: SocketAddr) -> io::Result<Self> {
        let sock = UdpSocket::bind(bind_addr)?;
        let local = sock.local_addr()?;
        Ok(Self {
            server: WebRtcServer::new(vec![local]),
            sock,
            local,
            events: mio::Events::with_capacity(256),
            last_seen: std::collections::HashMap::new(),
            idle_after: Duration::from_secs(60),
            pending: Vec::new(),
            schema_descriptor: Bytes::new(),
        })
    }

    /// Take ownership of an already-bound UDP socket. The host loop binds the
    /// game port itself (so the seat reservation can advertise the real port
    /// and LAN/public candidates can be announced) and hands the socket over.
    pub fn from_socket(sock: std::net::UdpSocket, announced_addrs: Vec<SocketAddr>) -> io::Result<Self> {
        sock.set_nonblocking(true)?;
        let local = sock.local_addr()?;
        Ok(Self {
            server: WebRtcServer::new(announced_addrs),
            sock: UdpSocket::from_std(sock),
            local,
            events: mio::Events::with_capacity(256),
            last_seen: std::collections::HashMap::new(),
            idle_after: Duration::from_secs(60),
            pending: Vec::new(),
            schema_descriptor: Bytes::new(),
        })
    }

    /// Read-only access to the underlying server (telemetry: is_connected).
    pub fn server(&self) -> &WebRtcServer {
        &self.server
    }

    /// Access the underlying server (signaling: handle_offer/candidate, the
    /// lane-classified inbound drain, stamped delta sends).
    pub fn server_mut(&mut self) -> &mut WebRtcServer {
        &mut self.server
    }

    /// Set the `openoxide.schema.bin` descriptor pushed to a session on its
    /// reliable lane as soon as that lane opens.
    pub fn set_schema_descriptor(&mut self, descriptor: impl Into<Bytes>) {
        self.schema_descriptor = descriptor.into();
    }

    /// Queue a schema state frame on the latest-wins unreliable lane (the
    /// delta lane's outbound queue replaces, not appends). A session with no
    /// open lane is a no-op here — the caller falls back to `send_to`.
    pub fn send_state(&mut self, client_id: &str, payload: &[u8]) -> io::Result<()> {
        let _ = self.server.send_delta(client_id, 0, 0, payload);
        self.flush()
    }

    /// Frames dropped because a session had no open channel to receive them.
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

impl GameTransport for WebRtcTransport {
    fn poll_events(&mut self, _timeout: Duration, events: &mut Vec<TransportEvent>) -> io::Result<()> {
        events.append(&mut self.pending);
        self.recv_udp();
        let now = Instant::now();

        self.server.tick(now);

        // Outgoing frames -> the UDP socket.
        for (dest, data) in self.server.poll_transmits() {
            let _ = self.sock.send_to(&data, dest);
        }

        // Session lifecycle -> Connected/Disconnected. The first open of a
        // session's reliable lane carries the schema descriptor, so the client
        // has its state contract before the first delta arrives.
        for ev in self.server.poll_events() {
            match ev {
                WebRtcServerEvent::Connected { client_id } => {
                    self.last_seen.insert(client_id.clone(), now);
                    if let Ok(peer) = self.sock.local_addr() {
                        self.pending.push(TransportEvent::Connected { client_id: client_id.into(), peer_addr: peer });
                    }
                }
                WebRtcServerEvent::ChannelOpen { client_id, label } => {
                    self.last_seen.insert(client_id.clone(), now);
                    if label == Lane::Reliable.label() && !self.schema_descriptor.is_empty() {
                        self.server.send_reliable(&client_id, &self.schema_descriptor);
                    }
                    if let Ok(peer) = self.sock.local_addr() {
                        self.pending.push(TransportEvent::Connected { client_id: client_id.into(), peer_addr: peer });
                    }
                }
                WebRtcServerEvent::Disconnected { client_id } => {
                    self.last_seen.remove(&client_id);
                    self.pending.push(TransportEvent::Disconnected { client_id: client_id.into() });
                }
                WebRtcServerEvent::Message { .. } => {}
            }
        }

        // Lane-classified inbound -> Packet events. Bytes that are protocol
        // control (acks, fragments) are consumed by the server itself and
        // never surface here.
        for (client_id, msg) in self.server.poll_inbound() {
            self.last_seen.insert(client_id.clone(), now);
            if let super::contract::Inbound::Input { payload, .. } = msg {
                self.pending.push(TransportEvent::Packet {
                    client_id: client_id.into(),
                    opcode: OpCode::PlayerInput,
                    payload: bytes::Bytes::copy_from_slice(&payload),
                });
            }
        }

        // Idle sweep: WebRTC sessions that stopped sending anything.
        // `remove_session` queues the `Disconnected` itself, so it surfaces on
        // the next poll — exactly once, like every other teardown.
        let idle_after = self.idle_after;
        let stale: Vec<String> = self
            .last_seen
            .iter()
            .filter(|(_, seen)| now.duration_since(**seen) > idle_after)
            .map(|(cid, _)| cid.clone())
            .collect();
        for cid in stale {
            self.server.remove_session(&cid);
            self.last_seen.remove(&cid);
        }

        events.append(&mut self.pending);
        Ok(())
    }

    fn send_to(&mut self, client_id: &str, opcode: OpCode, payload: &[u8]) -> io::Result<()> {
        // Events/acks ride the reliable lane; gameplay data the unreliable one.
        match opcode {
            OpCode::StateDelta | OpCode::StateSnapshot => {
                let _ = self.server.send_delta(client_id, 0, 0, payload);
            }
            _ => {
                self.server.send_reliable(client_id, payload);
            }
        }
        self.flush()
    }

    fn broadcast(&mut self, opcode: OpCode, payload: &[u8]) -> io::Result<()> {
        let lane = match opcode {
            OpCode::StateDelta | OpCode::StateSnapshot => Lane::Unreliable,
            _ => Lane::Reliable,
        };
        self.server.broadcast_frame(lane, payload);
        self.flush()
    }

    fn disconnect(&mut self, client_id: &str) -> io::Result<()> {
        // Only a session that actually existed reports a `Disconnected`: a
        // server-initiated close for an unknown id (a rejected join, a double
        // leave) must not fabricate a phantom disconnect.
        if self.server.remove_session(client_id) {
            self.last_seen.remove(client_id);
            self.pending.push(TransportEvent::Disconnected {
                client_id: client_id.into(),
            });
        }
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        for (dest, data) in self.server.poll_transmits() {
            let _ = self.sock.send_to(&data, dest);
        }
        Ok(())
    }

    fn ccu(&self) -> usize {
        self.last_seen.len()
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.local)
    }

    fn transport_name(&self) -> &'static str {
        "WebRTC DataChannel (UDP/str0m)"
    }
}

// Silence unused-import warning for `self.events` until the mio-driven
// variant (signaling over the same socket) lands with R2a completion.
#[allow(dead_code)]
fn _events_unused(_: &mio::Events) {}
