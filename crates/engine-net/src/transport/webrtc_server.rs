//! # WebRTC Server Transport (Sans-IO via str0m)
//!
//! Direct Browser-to-Server WebRTC DataChannels over UDP (Star Topology):
//! - `game-unreliable`: Unordered, 0-retransmits high-frequency channel for 50Hz–128Hz input & state deltas.
//! - `game-reliable`: Ordered reliable channel for critical game events (fire, damage, kills).
//!
//! Uses `str0m` with zero async runtime overhead, integrating directly into `mio` event loops.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Instant;

use str0m::change::SdpOffer;
use str0m::channel::ChannelId;
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, Input, Output, Rtc};

/// Events emitted by the WebRTC server to the game engine loop.
#[derive(Debug)]
pub enum WebRtcServerEvent {
    Connected {
        client_id: String,
    },
    Disconnected {
        client_id: String,
    },
    ChannelOpen {
        client_id: String,
        label: String,
    },
    Message {
        client_id: String,
        channel_label: String,
        data: Vec<u8>,
    },
}

/// A WebRTC peer session corresponding to one connected client.
pub struct WebRtcSession {
    pub client_id: String,
    pub rtc: Rtc,
    pub ch_unreliable: Option<ChannelId>,
    pub ch_reliable: Option<ChannelId>,
    pub channel_labels: HashMap<ChannelId, String>,
    pub remote_addr: Option<SocketAddr>,
    pub connected: bool,
    pub last_active: Instant,
    pub next_timeout: Option<Instant>,
}

impl WebRtcSession {
    pub fn new(client_id: String, rtc: Rtc) -> Self {
        Self {
            client_id,
            rtc,
            ch_unreliable: None,
            ch_reliable: None,
            channel_labels: HashMap::new(),
            remote_addr: None,
            connected: false,
            last_active: Instant::now(),
            next_timeout: None,
        }
    }
}

/// The WebRTC Server engine managing multiple client RTC sessions over UDP.
pub struct WebRtcServer {
    sessions: HashMap<String, WebRtcSession>,
    addr_to_session: HashMap<SocketAddr, String>,
    announced_candidates: Vec<Candidate>,
    pending_outputs: Vec<(SocketAddr, Vec<u8>)>,
    events: Vec<WebRtcServerEvent>,
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
            pending_outputs: Vec::new(),
            events: Vec::new(),
        }
    }

    /// Add an announced public or LAN host candidate address.
    pub fn add_announced_addr(&mut self, addr: SocketAddr) {
        if let Ok(cand) = Candidate::host(addr, Protocol::Udp) {
            self.announced_candidates.push(cand);
        }
    }

    /// Process an SDP Offer received from a client via WebSocket signaling.
    /// Creates a dedicated `str0m::Rtc` instance and returns the SDP Answer.
    pub fn handle_offer(&mut self, client_id: &str, sdp: &str) -> Result<String, String> {
        let offer = SdpOffer::from_sdp_string(sdp).map_err(|e| format!("invalid SDP offer: {e:?}"))?;
        let now = Instant::now();

        let mut rtc = Rtc::builder().build(now);
        for cand in &self.announced_candidates {
            let _ = rtc.add_local_candidate(cand.clone());
        }

        let answer = rtc
            .sdp_api()
            .accept_offer(offer)
            .map_err(|e| format!("failed to accept offer: {e:?}"))?;

        let answer_sdp = answer.to_sdp_string();

        let mut session = WebRtcSession::new(client_id.to_string(), rtc);
        Self::poll_session_events(&mut session, &mut self.events, &mut self.pending_outputs);

        self.sessions.insert(client_id.to_string(), session);
        Ok(answer_sdp)
    }

    /// Handle an ICE candidate sent by the client.
    pub fn handle_candidate(&mut self, client_id: &str, sdp_candidate: &str) -> Result<(), String> {
        let Some(session) = self.sessions.get_mut(client_id) else {
            return Err("session not found".to_string());
        };

        let trimmed = sdp_candidate.trim();
        let cand = Candidate::from_sdp_string(trimmed)
            .or_else(|_| {
                let stripped = trimmed.trim_start_matches("a=").trim();
                Candidate::from_sdp_string(stripped)
            })
            .or_else(|_| {
                let stripped = trimmed.trim_start_matches("a=").trim_start_matches("candidate:").trim();
                Candidate::from_sdp_string(stripped)
            });

        if let Ok(c) = cand {
            let _ = session.rtc.add_remote_candidate(c);
            Self::poll_session_events(session, &mut self.events, &mut self.pending_outputs);
        }
        Ok(())
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
                Self::poll_session_events(session, &mut self.events, &mut self.pending_outputs);
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
                Self::poll_session_events(session, &mut self.events, &mut self.pending_outputs);
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
                        Self::poll_session_events(session, &mut self.events, &mut self.pending_outputs);
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

    /// Periodic tick for DTLS timeouts and SCTP retransmissions.
    pub fn tick(&mut self, now: Instant) {
        for session in self.sessions.values_mut() {
            let should_tick = match session.next_timeout {
                Some(t) => now >= t,
                None => false,
            };
            if should_tick {
                let _ = session.rtc.handle_input(Input::Timeout(now));
                Self::poll_session_events(session, &mut self.events, &mut self.pending_outputs);
            }
        }
    }

    /// Send data on the unreliable unordered DataChannel (`game-unreliable`).
    pub fn send_unreliable(&mut self, client_id: &str, data: &[u8]) -> bool {
        let Some(session) = self.sessions.get_mut(client_id) else {
            return false;
        };
        let Some(cid) = session.ch_unreliable else {
            return false;
        };
        let Some(mut channel) = session.rtc.channel(cid) else {
            return false;
        };

        let ok = channel.write(true, data).is_ok();
        Self::poll_session_events(session, &mut self.events, &mut self.pending_outputs);
        ok
    }

    /// Send data on the reliable ordered DataChannel (`game-reliable`).
    pub fn send_reliable(&mut self, client_id: &str, data: &[u8]) -> bool {
        let Some(session) = self.sessions.get_mut(client_id) else {
            return false;
        };
        let Some(cid) = session.ch_reliable else {
            return false;
        };
        let Some(mut channel) = session.rtc.channel(cid) else {
            return false;
        };

        let ok = channel.write(true, data).is_ok();
        Self::poll_session_events(session, &mut self.events, &mut self.pending_outputs);
        ok
    }

    /// Broadcast data to all clients with an open `game-unreliable` channel.
    pub fn broadcast_unreliable(&mut self, data: &[u8]) -> usize {
        let mut count = 0;
        for session in self.sessions.values_mut() {
            if let Some(cid) = session.ch_unreliable {
                if let Some(mut channel) = session.rtc.channel(cid) {
                    if channel.write(true, data).is_ok() {
                        count += 1;
                    }
                }
            }
            Self::poll_session_events(session, &mut self.events, &mut self.pending_outputs);
        }
        count
    }

    /// Broadcast data to all clients with an open `game-reliable` channel.
    pub fn broadcast_reliable(&mut self, data: &[u8]) -> usize {
        let mut count = 0;
        for session in self.sessions.values_mut() {
            if let Some(cid) = session.ch_reliable {
                if let Some(mut channel) = session.rtc.channel(cid) {
                    if channel.write(true, data).is_ok() {
                        count += 1;
                    }
                }
            }
            Self::poll_session_events(session, &mut self.events, &mut self.pending_outputs);
        }
        count
    }

    /// Check if a client is connected over WebRTC.
    pub fn is_connected(&self, client_id: &str) -> bool {
        self.sessions
            .get(client_id)
            .map(|s| s.connected && s.ch_unreliable.is_some())
            .unwrap_or(false)
    }

    /// Number of active WebRTC client sessions.
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Remove a client session.
    pub fn remove_session(&mut self, client_id: &str) {
        if let Some(session) = self.sessions.remove(client_id) {
            if let Some(addr) = session.remote_addr {
                self.addr_to_session.remove(&addr);
            }
            self.events.push(WebRtcServerEvent::Disconnected {
                client_id: client_id.to_string(),
            });
        }
    }

    /// Drain queued events for the game loop.
    pub fn poll_events(&mut self) -> Vec<WebRtcServerEvent> {
        std::mem::take(&mut self.events)
    }

    /// Drain outgoing UDP datagrams to send via socket.
    pub fn poll_transmits(&mut self) -> Vec<(SocketAddr, Vec<u8>)> {
        std::mem::take(&mut self.pending_outputs)
    }

    fn poll_session_events(
        session: &mut WebRtcSession,
        events: &mut Vec<WebRtcServerEvent>,
        pending: &mut Vec<(SocketAddr, Vec<u8>)>,
    ) {
        while let Ok(output) = session.rtc.poll_output() {
            match output {
                Output::Transmit(send) => {
                    pending.push((send.destination, send.contents.to_vec()));
                }
                Output::Event(Event::Connected) => {
                    session.connected = true;
                    events.push(WebRtcServerEvent::Connected {
                        client_id: session.client_id.clone(),
                    });
                }
                Output::Event(Event::ChannelOpen(cid, label)) => {
                    session.channel_labels.insert(cid, label.clone());
                    if label.contains("unreliable") {
                        session.ch_unreliable = Some(cid);
                    } else if label.contains("reliable") {
                        session.ch_reliable = Some(cid);
                    }
                    events.push(WebRtcServerEvent::ChannelOpen {
                        client_id: session.client_id.clone(),
                        label,
                    });
                }
                Output::Event(Event::ChannelData(data)) => {
                    let label = session
                        .channel_labels
                        .get(&data.id)
                        .cloned()
                        .unwrap_or_else(|| "unknown".to_string());
                    events.push(WebRtcServerEvent::Message {
                        client_id: session.client_id.clone(),
                        channel_label: label,
                        data: data.data,
                    });
                }
                Output::Event(Event::ChannelClose(cid)) => {
                    if session.ch_unreliable == Some(cid) {
                        session.ch_unreliable = None;
                    }
                    if session.ch_reliable == Some(cid) {
                        session.ch_reliable = None;
                    }
                }
                Output::Timeout(t) => {
                    session.next_timeout = Some(t);
                    break;
                }
                _ => {}
            }
        }
    }
}
