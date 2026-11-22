use std::collections::VecDeque;

use engine_webrtc_protocol::is_newer;

use crate::plane::reliable::congestion::Congestion;
use crate::plane::reliable::consts::INITIAL_RTO;
use crate::plane::reliable::consts::MAX_INFLIGHT_EVENTS;
use crate::plane::reliable::consts::MAX_RELIABLE_BYTES;
use crate::plane::reliable::error::QueueError;

#[derive(Debug)]
pub struct Pending {
    pub(super) seq: u16,
    pub(super) wire: Vec<u8>,
    pub(super) since: u64,
    /// Time of the first transmission, used by RACK.
    pub(super) sent_at: Option<u64>,
    /// Time of the most recent transmission.
    pub(super) last_tx: u64,
    pub(super) due: u64,
    pub(super) attempts: u32,
    pub(super) sent_once: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReliableCounts {
    pub queued: u64,
    pub sent: u64,
    pub resent: u64,
    pub acked: u64,
    pub expired: u64,
    pub rejected: u64,
    pub bytes_sent: u64,
    pub bytes_acked: u64,
    pub loss_events: u64,
    pub pto_count: u64,
    pub rack_losses: u64,
    pub cwnd_bytes: u64,
    pub pacing_bps: u64,
}

#[derive(Debug)]
pub struct ReliableLane {
    pub(super) window: VecDeque<Pending>,
    pub(super) last_seq: Option<u16>,
    pub(super) congestion: Congestion,
    pub(super) counts: ReliableCounts,
}

impl Default for ReliableLane {
    fn default() -> Self {
        Self::new()
    }
}

impl ReliableLane {
    pub fn new() -> ReliableLane {
        ReliableLane {
            window: VecDeque::new(),
            last_seq: None,
            congestion: Congestion::new(),
            counts: ReliableCounts::default(),
        }
    }

    pub fn counts(&self) -> ReliableCounts {
        ReliableCounts {
            bytes_sent: self.congestion.bytes_sent,
            bytes_acked: self.congestion.bytes_acked,
            loss_events: self.congestion.loss_events,
            pto_count: self.congestion.pto_count,
            rack_losses: self.congestion.rack_losses,
            cwnd_bytes: self.congestion.cwnd_bytes() as u64,
            pacing_bps: self.congestion.pacing_bps as u64,
            ..self.counts
        }
    }

    pub fn inflight(&self) -> usize {
        self.window.len()
    }

    pub fn inflight_bytes(&self) -> usize {
        self.window.iter().map(|entry| entry.wire.len()).sum()
    }

    pub fn rtt_ms(&self) -> f64 {
        self.congestion.srtt_ms
    }

    /// Legacy retransmission timeout estimate retained for callers that report
    /// it. The pump uses the RFC 6298 PTO below.
    pub fn rto(&self) -> u64 {
        if self.congestion.srtt_ms > 0.0 {
            Self::rto_from(self.congestion.srtt_ms)
        } else {
            INITIAL_RTO
        }
    }

    /// RFC 6298 probe timeout (SRTT + 4*RTTVAR, clamped).
    pub fn pto(&self) -> u64 {
        self.congestion.pto_ms
    }

    pub fn cwnd(&self) -> usize {
        self.congestion.cwnd_events()
    }

    pub fn cwnd_bytes(&self) -> usize {
        self.congestion.cwnd_bytes()
    }

    pub fn pacing_bps(&self) -> f64 {
        self.congestion.pacing_bps
    }

    pub fn next_due(&self) -> Option<u64> {
        self.window.front().map(|entry| entry.due)
    }

    pub fn queue(&mut self, seq: u16, wire: Vec<u8>, now_ms: u64) -> Result<(), QueueError> {
        if wire.is_empty() || wire.len() > MAX_RELIABLE_BYTES {
            self.counts.rejected += 1;
            return Err(QueueError::TooBig { bytes: wire.len() });
        }
        if let Some(last) = self.last_seq
            && !is_newer(last, seq)
        {
            self.counts.rejected += 1;
            return Err(QueueError::NotNewer { seq, last });
        }
        let limit = self.cwnd().min(MAX_INFLIGHT_EVENTS).max(1);
        if self.window.len() >= limit
            || self.inflight_bytes() + wire.len() > self.congestion.cwnd_bytes()
        {
            self.counts.rejected += 1;
            return Err(QueueError::WindowFull {
                inflight: self.window.len(),
            });
        }
        self.last_seq = Some(seq);
        self.counts.queued += 1;
        self.window.push_back(Pending {
            seq,
            wire,
            since: now_ms,
            sent_at: None,
            last_tx: now_ms,
            due: now_ms,
            attempts: 0,
            sent_once: false,
        });
        Ok(())
    }

    pub(super) fn on_ack_sample(&mut self, sample_ms: f64, now_ms: u64, bytes: usize) {
        self.congestion.on_ack(sample_ms, now_ms, bytes);
    }

    pub(super) fn on_send(&mut self, bytes: usize, now_ms: u64) {
        self.congestion.on_send(bytes, now_ms);
    }

    pub(super) fn congestion_event(&mut self, now_ms: u64) {
        self.congestion.on_retransmit(now_ms);
    }
}
