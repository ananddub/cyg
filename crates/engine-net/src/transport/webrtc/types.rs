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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TransportStats {
    pub frames_sent: u64,

    pub frames_queued: u64,

    pub deltas_sent: u64,

    pub deltas_refused_oversized: u64,

    pub deltas_dropped_saturated: u64,

    pub frames_dropped_saturated: u64,

    pub deltas_superseded: u64,

    pub frames_refused_oversize: u64,

    pub frames_dropped_no_channel: u64,

    pub write_errors: u64,
    pub inputs_received: u64,

    pub inputs_stale: u64,
    pub events_received: u64,
    pub acks_received: u64,

    pub acks_stale: u64,

    pub frames_misrouted: u64,

    pub frames_malformed: u64,

    pub frames_off_contract: u64,

    pub contract_violations: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WriteAttempt {
    Accepted,

    Saturated,

    Lost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeltaSendReport {
    pub frames_accepted: usize,

    pub frames_dropped: usize,

    pub fragments: usize,

    pub superseded_queued: usize,
}

impl DeltaSendReport {
    pub(super) fn skipped(fragments: usize) -> Self {
        Self {
            frames_accepted: 0,
            frames_dropped: fragments,
            fragments,
            superseded_queued: 0,
        }
    }

    pub fn delivered(&self) -> bool {
        self.frames_accepted == self.fragments && self.frames_dropped == 0
    }
}
