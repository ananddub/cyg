use super::WebRtcServer;
use super::contract::{FRAME_MAX_WIRE_LEN, Lane};

impl WebRtcServer {
    pub(super) fn send_frame(&mut self, lane: Lane, client_id: &str, data: &[u8]) -> bool {
        if data.len() > FRAME_MAX_WIRE_LEN {
            self.sink.stats.frames_refused_oversize += 1;
            tracing::error!(
                "[webrtc] ⚠️ refused {} B on {} — MTU budget is {} B, split it (send_delta)",
                data.len(),
                lane.label(),
                FRAME_MAX_WIRE_LEN
            );
            return false;
        }
        let Some(session) = self.sessions.get_mut(client_id) else {
            return false;
        };
        let sent = Self::write_or_queue(session, lane, data, None, None, &mut self.sink);
        Self::settle(session, &mut self.sink);
        sent
    }

    pub(super) fn broadcast_frame(&mut self, lane: Lane, data: &[u8]) -> usize {
        if data.len() > FRAME_MAX_WIRE_LEN {
            self.sink.stats.frames_refused_oversize += 1;
            return 0;
        }
        let mut count = 0;
        for session in self.sessions.values_mut() {
            if Self::write_or_queue(session, lane, data, None, None, &mut self.sink) {
                count += 1;
            }
            Self::settle(session, &mut self.sink);
        }
        count
    }

    pub(super) fn flush_outbound(&mut self) {
        for session in self.sessions.values_mut() {
            for lane in [Lane::Unreliable, Lane::Reliable] {
                Self::drain_queue(session, lane, &mut self.sink);
            }
            Self::settle(session, &mut self.sink);
        }
    }
}
