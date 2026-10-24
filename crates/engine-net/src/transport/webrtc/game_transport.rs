//! # WebRTC GameTransport adapter
//!
//! Wraps [`WebRtcServer`] + its UDP socket behind the [`GameTransport`] trait —
//! the same surface the old WS fallback exposed, so the host loop is
//! transport-agnostic. This is the R2a work item: once the host loop runs on
//! this, `transport/ws/` is deleted and the data plane is WebRTC + UDP only.

use std::io;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

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
        })
    }

    /// Access the underlying server (signaling: handle_offer/candidate).
    pub fn server(&mut self) -> &mut WebRtcServer {
        &mut self.server
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

        // Session lifecycle -> Connected/Disconnected.
        for ev in self.server.poll_events() {
            match ev {
                WebRtcServerEvent::Connected { client_id }
                | WebRtcServerEvent::ChannelOpen { client_id, .. } => {
                    self.last_seen.insert(client_id.clone(), now);
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
            self.pending.push(TransportEvent::Disconnected { client_id: cid.into() });
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
        self.server.remove_session(client_id);
        self.last_seen.remove(client_id);
        self.pending.push(TransportEvent::Disconnected {
            client_id: client_id.into(),
        });
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
