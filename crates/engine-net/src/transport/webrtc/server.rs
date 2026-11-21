use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use str0m::net::Protocol;
use str0m::{Candidate, Input};

use super::contract::Inbound;
use super::session::{Sink, WebRtcSession};
use super::types::WebRtcServerEvent;

pub struct WebRtcServer {
    pub(super) sessions: HashMap<String, WebRtcSession>,
    pub(super) addr_to_session: HashMap<SocketAddr, String>,
    pub(super) announced_candidates: Vec<Candidate>,
    pub(super) sink: Sink,
    /// Last time an unknown-source datagram was allowed through the
    /// O(sessions) `rtc.accepts` scan. Everything in between is dropped.
    pub(super) last_unknown_sweep: Option<Instant>,
    /// Unknown-source datagrams dropped between sweeps.
    pub(super) unknown_dropped: u64,
}

/// How often an unknown-source datagram triggers the session scan; cheap
/// rate limit against UDP source-address flooding.
const UNKNOWN_SWEEP_INTERVAL: Duration = Duration::from_millis(250);

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
            last_unknown_sweep: None,
            unknown_dropped: 0,
        }
    }

    pub fn unknown_dropped(&self) -> u64 {
        self.unknown_dropped
    }

    /// Whether an unknown-source datagram may run the (O(sessions)) accepts
    /// scan now; otherwise it is dropped and counted.
    pub fn admit_unknown_source(&mut self, now: Instant) -> bool {
        let allowed = self
            .last_unknown_sweep
            .is_none_or(|last| now.duration_since(last) >= UNKNOWN_SWEEP_INTERVAL);
        if allowed {
            self.last_unknown_sweep = Some(now);
        } else {
            self.unknown_dropped += 1;
        }
        allowed
    }

    pub fn add_announced_addr(&mut self, addr: SocketAddr) {
        if let Ok(cand) = Candidate::host(addr, Protocol::Udp) {
            self.announced_candidates.push(cand);
        }
    }

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

    pub fn poll_events(&mut self) -> Vec<WebRtcServerEvent> {
        std::mem::take(&mut self.sink.events)
    }

    pub fn poll_inbound(&mut self) -> Vec<(String, Inbound)> {
        std::mem::take(&mut self.sink.inbound)
    }

    pub fn poll_transmits(&mut self) -> Vec<(SocketAddr, Vec<u8>)> {
        self.flush_outbound();
        std::mem::take(&mut self.sink.pending)
    }
}
