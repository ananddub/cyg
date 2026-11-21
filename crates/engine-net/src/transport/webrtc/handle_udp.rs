use std::net::SocketAddr;
use std::time::Instant;

use str0m::Input;
use str0m::net::{Protocol, Receive};

use super::WebRtcServer;

impl WebRtcServer {
    pub fn handle_udp(&mut self, from: SocketAddr, mut local: SocketAddr, packet: &[u8]) {
        let now = Instant::now();

        if local.ip().is_unspecified() {
            if from.ip().is_loopback() {
                local = SocketAddr::new(
                    std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                    local.port(),
                );
            } else if let Some(cand) = self
                .announced_candidates
                .iter()
                .find(|c| !c.addr().ip().is_loopback())
            {
                local = cand.addr();
            } else if let Some(cand) = self.announced_candidates.first() {
                local = cand.addr();
            }
        }

        let Ok(recv) = Receive::new(Protocol::Udp, from, local, packet) else {
            return;
        };
        let input = Input::Receive(now, recv);

        if let Some(client_id) = self.addr_to_session.get(&from) {
            if let Some(session) = self.sessions.get_mut(client_id) {
                session.last_active = now;
                let _ = session.rtc.handle_input(input);
                Self::poll_session_events(session, &mut self.sink);
                return;
            }
        }

        // Unknown source: the O(sessions) `rtc.accepts` scan is rate limited
        // so a flood of spoofed datagrams cannot burn a full scan per packet.
        if !self.admit_unknown_source(now) {
            return;
        }

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
}
