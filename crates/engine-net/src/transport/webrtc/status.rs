//! Read-only views a host uses to judge a client or the transport: seq
//! watermarks, backlog, connectivity and the contract counters.

use super::WebRtcServer;
use super::contract::Lane;
use super::types::TransportStats;

impl WebRtcServer {
    /// The newest input seq seen from this client, wrap-safe and forward-only.
    /// This is what the host echoes back as `acked_seq`.
    pub fn last_input_seq(&self, client_id: &str) -> Option<u16> {
        self.sessions
            .get(client_id)
            .and_then(|s| s.last_input_seq.get())
    }

    /// The newest event seq this client has acked on the reliable lane.
    pub fn acked_event_seq(&self, client_id: &str) -> Option<u16> {
        self.sessions
            .get(client_id)
            .and_then(|s| s.acked_event_seq.get())
    }

    /// Frames both lanes are still holding for this client: a slow-client
    /// signal a host can meter without walking the sessions itself.
    pub fn outbound_backlog(&self, client_id: &str) -> usize {
        self.sessions
            .get(client_id)
            .map(|s| s.queue_len(Lane::Unreliable) + s.queue_len(Lane::Reliable))
            .unwrap_or(0)
    }

    /// Check if a client is connected over WebRTC.
    pub fn is_connected(&self, client_id: &str) -> bool {
        self.sessions
            .get(client_id)
            .map(|s| s.connected && s.ch_unreliable.is_some())
            .unwrap_or(false)
    }

    /// Contract counters, for metrics and for telling a dead transport apart
    /// from an idle room.
    pub fn stats(&self) -> TransportStats {
        self.sink.stats
    }

    /// Number of active WebRTC client sessions.
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Frames dropped because a session had no open channel to receive them.
    pub fn dropped_frames(&self) -> u64 {
        self.sink.stats.frames_dropped_no_channel
    }
}
