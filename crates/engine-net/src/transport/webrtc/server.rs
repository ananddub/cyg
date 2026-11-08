use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Instant;

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
