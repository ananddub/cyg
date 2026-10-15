//! Events, counters and reports the WebRTC transport exposes to the game loop.

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

/// Contract counters for one transport instance. Every field distinguishes
/// "the room is quiet" from "the transport is throwing gameplay away".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TransportStats {
    /// Frames handed to SCTP for transmission.
    pub frames_sent: u64,
    /// Frames held in a lane queue because SCTP's send buffer was full.
    pub frames_queued: u64,
    /// Deltas whose every frame reached the lane (written or queued).
    pub deltas_sent: u64,
    /// Deltas refused because the body needed more than `MAX_FRAGMENTS` chunks.
    pub deltas_refused_oversized: u64,
    /// Deltas discarded under backpressure, oldest first.
    pub deltas_dropped_saturated: u64,
    /// Frames discarded with those deltas.
    pub frames_dropped_saturated: u64,
    /// Queued deltas replaced by a newer one on the unreliable lane.
    pub deltas_superseded: u64,
    /// Writes bigger than the path MTU, refused instead of truncated.
    pub frames_refused_oversize: u64,
    /// Frames discarded because the lane had no open channel.
    pub frames_dropped_no_channel: u64,
    /// `str0m` write errors on a channel id that was open.
    pub write_errors: u64,
    pub inputs_received: u64,
    /// Inputs whose seq the high-water mark had already passed. Still reported
    /// to the host, because rollback replay wants genuinely late inputs rather
    /// than dropping them.
    pub inputs_stale: u64,
    pub events_received: u64,
    pub acks_received: u64,
    /// Acks that did not move the forward-only high-water mark.
    pub acks_stale: u64,
    /// Frames refused for arriving on the other lane.
    pub frames_misrouted: u64,
    /// Frames refused for a bad length or impossible geometry.
    pub frames_malformed: u64,
    /// Traffic on a channel that is not part of the contract.
    pub frames_off_contract: u64,
    /// Channels mapped through a legacy alias label.
    pub legacy_alias_channels: u64,
    /// Channels whose negotiated SCTP reliability does not match their lane.
    pub contract_violations: u64,
}

/// Outcome of one attempt to hand a frame to `str0m`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WriteAttempt {
    Accepted,
    /// SCTP's send buffer is full: hold the rest and retry on the next drain.
    Saturated,
    /// The channel is gone; nothing queued behind it can be delivered either.
    Lost,
}

/// What one session's delta send actually achieved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeltaSendReport {
    /// Frames written to SCTP or accepted into this session's queue.
    pub frames_accepted: usize,
    /// Frames discarded because the lane was unusable or the queue could not
    /// hold them.
    pub frames_dropped: usize,
    /// How many frames this delta needed.
    pub fragments: usize,
    /// Older queued deltas this one replaced.
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

    /// Every frame reached the lane. Anything else means this client did not
    /// get this tick, and the host should use its reliable fallback — the same
    /// invariant `engine_server::netcode::send_delta_framed` reports as a frame
    /// count where `0` means not delivered.
    pub fn delivered(&self) -> bool {
        self.frames_accepted == self.fragments && self.frames_dropped == 0
    }
}
