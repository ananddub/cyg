
use super::WebRtcServer;
use super::contract::Lane;
use super::types::TransportStats;

impl WebRtcServer {

    pub fn last_input_seq(&self, client_id: &str) -> Option<u16> {
        self.sessions
            .get(client_id)
            .and_then(|s| s.last_input_seq.get())
    }

    pub fn acked_event_seq(&self, client_id: &str) -> Option<u16> {
        self.sessions
            .get(client_id)
            .and_then(|s| s.acked_event_seq.get())
    }

    pub fn outbound_backlog(&self, client_id: &str) -> usize {
        self.sessions
            .get(client_id)
            .map(|s| s.queue_len(Lane::Unreliable) + s.queue_len(Lane::Reliable))
            .unwrap_or(0)
    }

    pub fn is_connected(&self, client_id: &str) -> bool {
        self.sessions
            .get(client_id)
            .map(|s| s.connected && s.ch_unreliable.is_some())
            .unwrap_or(false)
    }

    pub fn stats(&self) -> TransportStats {
        self.sink.stats
    }

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    pub fn dropped_frames(&self) -> u64 {
        self.sink.stats.frames_dropped_no_channel
    }
}
