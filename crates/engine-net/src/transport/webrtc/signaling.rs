use std::time::Instant;

use str0m::change::SdpOffer;
use str0m::{Candidate, Rtc};

use super::WebRtcServer;
use super::session::WebRtcSession;

impl WebRtcServer {
    pub fn handle_offer(&mut self, client_id: &str, sdp: &str) -> Result<String, String> {
        let offer =
            SdpOffer::from_sdp_string(sdp).map_err(|e| format!("invalid SDP offer: {e:?}"))?;
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

        if let Some(mut old) = self.sessions.remove(client_id) {
            if let Some(addr) = old.remote_addr {
                self.addr_to_session.remove(&addr);
            }
            self.sink.stats.frames_dropped_no_channel += old.discard_outbound() as u64;
        }

        let mut session = WebRtcSession::new(client_id.to_string(), rtc);
        Self::poll_session_events(&mut session, &mut self.sink);

        self.sessions.insert(client_id.to_string(), session);
        Ok(answer_sdp)
    }

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
                let stripped = trimmed
                    .trim_start_matches("a=")
                    .trim_start_matches("candidate:")
                    .trim();
                Candidate::from_sdp_string(stripped)
            });

        if let Ok(c) = cand {
            let _ = session.rtc.add_remote_candidate(c);
            Self::poll_session_events(session, &mut self.sink);
        }
        Ok(())
    }
}
