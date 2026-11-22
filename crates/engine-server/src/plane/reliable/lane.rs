use std::collections::VecDeque;

use engine_webrtc_protocol::is_newer;

use crate::plane::reliable::consts::INITIAL_CWND;
use crate::plane::reliable::consts::INITIAL_RTO;
use crate::plane::reliable::consts::MAX_INFLIGHT_EVENTS;
use crate::plane::reliable::consts::MAX_RELIABLE_BYTES;
use crate::plane::reliable::consts::MIN_CWND;
use crate::plane::reliable::error::QueueError;

#[derive(Debug)]
pub struct Pending {
    pub(super) seq: u16,
    pub(super) wire: Vec<u8>,
    pub(super) since: u64,
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
}

#[derive(Debug)]
pub struct ReliableLane {
    pub(super) window: VecDeque<Pending>,
    pub(super) last_seq: Option<u16>,
    pub(super) srtt_ms: f64,
    pub(super) rto: u64,
    /// AIMD congestion window in events. Starts at the hard cap, halves on a
    /// retransmit (one reduction per RTT), and grows on every ack.
    pub(super) cwnd: f64,
    pub(super) ssthresh: f64,
    pub(super) last_loss_ms: Option<u64>,
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
            srtt_ms: 0.0,
            rto: INITIAL_RTO,
            cwnd: INITIAL_CWND,
            ssthresh: INITIAL_CWND,
            last_loss_ms: None,
            counts: ReliableCounts::default(),
        }
    }

    pub fn counts(&self) -> ReliableCounts {
        self.counts
    }

    pub fn inflight(&self) -> usize {
        self.window.len()
    }

    pub fn rtt_ms(&self) -> f64 {
        self.srtt_ms
    }

    pub fn rto(&self) -> u64 {
        self.rto
    }

    /// Current congestion window in events (integer view for callers).
    pub fn cwnd(&self) -> usize {
        self.cwnd.max(MIN_CWND).floor() as usize
    }

    /// Shrink the window on a retransmit. At most once per RTO so one loss
    /// burst cannot collapse it to one event.
    pub(super) fn congestion_event(&mut self, now_ms: u64) {
        if let Some(last) = self.last_loss_ms
            && now_ms.saturating_sub(last) < self.rto
        {
            return;
        }
        self.ssthresh = (self.cwnd * 0.5).max(MIN_CWND);
        self.cwnd = self.ssthresh;
        self.last_loss_ms = Some(now_ms);
    }

    /// Grow after acknowledged events: slow start below `ssthresh`, then one
    /// event per window (the classic AIMD increase).
    pub(super) fn on_ack(&mut self, released: usize) {
        for _ in 0..released {
            if self.cwnd < self.ssthresh {
                self.cwnd += 1.0;
            } else {
                self.cwnd += 1.0 / self.cwnd.max(1.0);
            }
            if self.cwnd > MAX_INFLIGHT_EVENTS as f64 {
                self.cwnd = MAX_INFLIGHT_EVENTS as f64;
            }
        }
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
        if self.window.len() >= limit {
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
            last_tx: now_ms,
            due: now_ms,
            attempts: 0,
            sent_once: false,
        });
        Ok(())
    }
}
