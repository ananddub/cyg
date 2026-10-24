//! The transport itself: session bookkeeping, UDP ingress and the output drains
//! the game loop polls.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Instant;

use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Input};

use super::contract::Inbound;
use super::session::{Sink, WebRtcSession};
use super::types::WebRtcServerEvent;

/// The WebRTC Server engine managing multiple client RTC sessions over UDP.
pub struct WebRtcServer {
    pub(super) sessions: HashMap<String, WebRtcSession>,
    pub(super) addr_to_session: HashMap<SocketAddr, String>,
    pub(super) announced_candidates: Vec<Candidate>,
    pub(super) sink: Sink,
}

impl WebRtcServer {
    pub fn new(announced_addrs: Vec<SocketAddr>) -> Self {
        let mut announced_candidates = Vec::new();
        for addr in announced_addrs {
            if let Ok(cand) = Candidate::host(addr, Protocol::Udp) {
                announced_candidates.push(cand);
            }
        }
        Self {
            sessions: HashMap::new(),
            addr_to_session: HashMap::new(),
            announced_candidates,
            sink: Sink::default(),
        }
    }

    /// Add an announced public or LAN host candidate address.
    pub fn add_announced_addr(&mut self, addr: SocketAddr) {
        if let Ok(cand) = Candidate::host(addr, Protocol::Udp) {
            self.announced_candidates.push(cand);
        }
    }

    /// Ingest an incoming UDP packet received on the server's UDP socket.
    pub fn handle_udp(&mut self, from: SocketAddr, mut local: SocketAddr, packet: &[u8]) {
        let now = Instant::now();

        // If socket was bound to 0.0.0.0, resolve local destination to matching announced candidate
        if local.ip().is_unspecified() {
            if from.ip().is_loopback() {
                local = SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), local.port());
            } else if let Some(cand) = self.announced_candidates.iter().find(|c| !c.addr().ip().is_loopback()) {
                local = cand.addr();
            } else if let Some(cand) = self.announced_candidates.first() {
                local = cand.addr();
            }
        }

        let Ok(recv) = Receive::new(Protocol::Udp, from, local, packet) else {
            return;
        };
        let input = Input::Receive(now, recv);

        // 1. Fast path: check known remote address mapping
        if let Some(client_id) = self.addr_to_session.get(&from) {
            if let Some(session) = self.sessions.get_mut(client_id) {
                session.last_active = now;
                let _ = session.rtc.handle_input(input);
                Self::poll_session_events(session, &mut self.sink);
                return;
            }
        }

        // 2. Slow path: check which RTC session accepts this packet (ICE / STUN binding)
        let mut matched_client_id: Option<String> = None;
        for (client_id, session) in &mut self.sessions {
            if session.rtc.accepts(&input) {
                session.last_active = now;
                session.remote_addr = Some(from);
                let _ = session.rtc.handle_input(input);
                Self::poll_session_events(session, &mut self.sink);
                matched_client_id = Some(client_id.clone());
                break;
            }

            // Fallback: try all announced candidates as destination
            let mut accepted = false;
            for cand in &self.announced_candidates {
                if let Ok(cand_recv) = Receive::new(Protocol::Udp, from, cand.addr(), packet) {
                    let cand_input = Input::Receive(now, cand_recv);
                    if session.rtc.accepts(&cand_input) {
                        session.last_active = now;
                        session.remote_addr = Some(from);
                        let _ = session.rtc.handle_input(cand_input);
                        Self::poll_session_events(session, &mut self.sink);
                        matched_client_id = Some(client_id.clone());
                        accepted = true;
                        break;
                    }
                }
            }
            if accepted {
                break;
            }
        }

        if let Some(client_id) = matched_client_id {
            self.addr_to_session.insert(from, client_id);
        }
    }

    /// Periodic tick for DTLS timeouts and SCTP retransmissions. Also the retry
    /// point for anything a full send buffer made a session hold.
    pub fn tick(&mut self, now: Instant) {
        for session in self.sessions.values_mut() {
            let should_tick = match session.next_timeout {
                Some(t) => now >= t,
                None => false,
            };
            if should_tick {
                let _ = session.rtc.handle_input(Input::Timeout(now));
                Self::poll_session_events(session, &mut self.sink);
            }
        }
        self.flush_outbound();
    }

    /// Remove a client session. `true` when a session actually went away; a
    /// miss is a no-op (no phantom `Disconnected` for an id never seen).
    pub fn remove_session(&mut self, client_id: &str) -> bool {
        if let Some(mut session) = self.sessions.remove(client_id) {
            if let Some(addr) = session.remote_addr {
                self.addr_to_session.remove(&addr);
            }
            self.sink.stats.frames_dropped_no_channel += session.discard_outbound() as u64;
            self.sink.events.push(WebRtcServerEvent::Disconnected {
                client_id: client_id.to_string(),
            });
            true
        } else {
            false
        }
    }

    /// Drain queued events for the game loop.
    pub fn poll_events(&mut self) -> Vec<WebRtcServerEvent> {
        std::mem::take(&mut self.sink.events)
    }

    /// Drain client frames the transport has already classified against their
    /// lane, with the high-water-mark result attached: inputs know whether they
    /// are resupplies, acks are already parsed. [`Self::poll_events`] still
    /// carries the same bytes for hosts that parse them themselves.
    pub fn poll_inbound(&mut self) -> Vec<(String, Inbound)> {
        std::mem::take(&mut self.sink.inbound)
    }

    /// Drain outgoing UDP datagrams to send via socket. Queued frames are
    /// retried first, so a lane that was full last tick drains in arrival order.
    pub fn poll_transmits(&mut self) -> Vec<(SocketAddr, Vec<u8>)> {
        self.flush_outbound();
        std::mem::take(&mut self.sink.pending)
    }
}
