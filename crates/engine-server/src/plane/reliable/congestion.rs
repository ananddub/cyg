//! AIMD congestion control + packet pacing for the reliable UDP lane.
//!
//! This is deliberately a small, testable subset of the loss-recovery ideas in
//! Gaffer's *reliability, ordering and congestion avoidance over UDP* and
//! RFC 9002: a byte-based congestion window, RACK-style loss detection from
//! selective acknowledgements, and a token bucket that spreads a burst over
//! one RTT instead of dumping it into the socket.

use crate::plane::reliable::consts::INITIAL_CWND_BYTES;
use crate::plane::reliable::consts::INITIAL_RTO;
use crate::plane::reliable::consts::MAX_CWND_BYTES;
use crate::plane::reliable::consts::MAX_RELIABLE_BYTES;
use crate::plane::reliable::consts::MAX_RTO;
use crate::plane::reliable::consts::MIN_CWND_BYTES;
use crate::plane::reliable::consts::MIN_RTO;

/// Pacing gain: send a little faster than the congestion window per RTT so a
/// paced burst can actually fill the window without waiting forever.
const PACING_GAIN: f64 = 2.0;
/// RFC 6298 clock granularity (ms).
const CLOCK_GRANULARITY_MS: f64 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Congestion {
    pub cwnd_bytes: f64,
    pub ssthresh_bytes: f64,
    pub srtt_ms: f64,
    pub rttvar_ms: f64,
    pub min_rtt_ms: f64,
    pub latest_rtt_ms: f64,
    pub pto_ms: u64,
    pub pacing_bps: f64,
    pub(super) tokens: f64,
    pub(super) last_refill_ms: Option<u64>,
    pub last_loss_ms: Option<u64>,
    pub loss_events: u64,
    pub pto_count: u64,
    pub rack_losses: u64,
    pub bytes_sent: u64,
    pub bytes_acked: u64,
}

impl Default for Congestion {
    fn default() -> Self {
        Self::new()
    }
}

impl Congestion {
    pub fn new() -> Self {
        Self {
            cwnd_bytes: INITIAL_CWND_BYTES,
            ssthresh_bytes: INITIAL_CWND_BYTES,
            srtt_ms: 0.0,
            rttvar_ms: 0.0,
            min_rtt_ms: 0.0,
            latest_rtt_ms: 0.0,
            pto_ms: INITIAL_RTO,
            pacing_bps: 0.0,
            tokens: INITIAL_CWND_BYTES,
            last_refill_ms: None,
            last_loss_ms: None,
            loss_events: 0,
            pto_count: 0,
            rack_losses: 0,
            bytes_sent: 0,
            bytes_acked: 0,
        }
    }

    /// Congestion window expressed in the lane's historical event units.
    pub fn cwnd_events(&self) -> usize {
        (self.cwnd_bytes / MAX_RELIABLE_BYTES as f64).floor() as usize
    }

    pub fn cwnd_bytes(&self) -> usize {
        self.cwnd_bytes.max(MIN_CWND_BYTES as f64).floor() as usize
    }

    pub fn tokens(&self) -> f64 {
        self.tokens
    }

    /// RFC 6298 sample update, plus the RFC 9002 PTO estimate.
    pub fn on_ack(&mut self, sample_ms: f64, now_ms: u64, acked_bytes: usize) {
        if sample_ms > 0.0 {
            self.latest_rtt_ms = sample_ms;
            if self.srtt_ms <= 0.0 {
                self.srtt_ms = sample_ms;
                self.rttvar_ms = sample_ms * 0.5;
                self.min_rtt_ms = sample_ms;
            } else {
                self.rttvar_ms = self.rttvar_ms * 0.75 + (self.srtt_ms - sample_ms).abs() * 0.25;
                self.srtt_ms = self.srtt_ms * 0.875 + sample_ms * 0.125;
                self.min_rtt_ms = self.min_rtt_ms.min(sample_ms);
            }
            self.pto_ms = Self::pto_from(self.srtt_ms, self.rttvar_ms);
        }
        self.bytes_acked = self.bytes_acked.saturating_add(acked_bytes as u64);
        if acked_bytes > 0 {
            self.grow(acked_bytes);
        }
        self.refill(now_ms);
    }

    /// Multiplicative decrease. One loss event per PTO so a burst of
    /// retransmits cannot collapse the window once per packet.
    pub fn on_retransmit(&mut self, now_ms: u64) {
        if let Some(last) = self.last_loss_ms
            && now_ms.saturating_sub(last) < self.pto_ms
        {
            return;
        }
        self.ssthresh_bytes = (self.cwnd_bytes * 0.5).max(MIN_CWND_BYTES as f64);
        self.cwnd_bytes = self.ssthresh_bytes;
        self.last_loss_ms = Some(now_ms);
        self.loss_events = self.loss_events.saturating_add(1);
        self.pto_count = self.pto_count.saturating_add(1);
        self.tokens = self.tokens.min(self.cwnd_bytes);
        self.refresh_rate();
    }

    /// RACK: a newer transmission was acknowledged, so an older transmission
    /// that has been outstanding longer than the reordering window is lost.
    /// Returns the number of packets newly marked lost.
    pub fn detect_rack_loss(
        &mut self,
        newest_acked_tx_ms: u64,
        now_ms: u64,
        pending: &mut [crate::plane::reliable::lane::Pending],
    ) -> usize {
        let reorder = (self.min_rtt_ms / 4.0).max(1.0) as u64;
        let mut lost = 0;
        for entry in pending.iter_mut() {
            if !entry.sent_once || entry.last_tx >= newest_acked_tx_ms {
                continue;
            }
            if newest_acked_tx_ms.saturating_sub(entry.last_tx) <= reorder {
                continue;
            }
            if entry.due <= now_ms {
                continue;
            }
            entry.due = now_ms;
            lost += 1;
        }
        if lost > 0 {
            self.rack_losses = self.rack_losses.saturating_add(lost as u64);
            self.loss_events = self.loss_events.saturating_add(1);
            self.on_retransmit(now_ms);
        }
        lost
    }

    pub fn on_send(&mut self, bytes: usize, now_ms: u64) {
        self.refill(now_ms);
        self.tokens = (self.tokens - bytes as f64).max(0.0);
        self.bytes_sent = self.bytes_sent.saturating_add(bytes as u64);
        self.refresh_rate();
    }

    pub fn can_send(&mut self, bytes: usize, now_ms: u64) -> bool {
        self.refill(now_ms);
        self.tokens + f64::EPSILON >= bytes as f64
    }

    /// Refill the token bucket from the last observation. With no RTT sample
    /// yet the bucket is simply kept full, which preserves the old
    /// "send the first burst immediately" behaviour.
    pub fn refill(&mut self, now_ms: u64) {
        let last = self.last_refill_ms.unwrap_or(now_ms);
        let elapsed = now_ms.saturating_sub(last) as f64;
        self.last_refill_ms = Some(now_ms);
        if self.srtt_ms <= 0.0 || self.pacing_bps <= 0.0 {
            self.tokens = self.cwnd_bytes;
            return;
        }
        self.tokens = (self.tokens + elapsed * self.pacing_bps / 1000.0).min(self.cwnd_bytes);
    }

    fn grow(&mut self, acked_bytes: usize) {
        let step = if self.cwnd_bytes < self.ssthresh_bytes {
            MAX_RELIABLE_BYTES as f64
        } else {
            (MAX_RELIABLE_BYTES * MAX_RELIABLE_BYTES) as f64 / self.cwnd_bytes.max(1.0)
        };
        let scaled = (step * acked_bytes as f64 / MAX_RELIABLE_BYTES as f64).max(1.0);
        self.cwnd_bytes = (self.cwnd_bytes + scaled).min(MAX_CWND_BYTES as f64);
        self.refresh_rate();
    }

    fn refresh_rate(&mut self) {
        let rtt = self.srtt_ms.max(MIN_RTO as f64);
        self.pacing_bps = self.cwnd_bytes / rtt * 1000.0 * PACING_GAIN;
    }

    pub fn pto_from(srtt: f64, rttvar: f64) -> u64 {
        (srtt + (4.0 * rttvar).max(CLOCK_GRANULARITY_MS)).clamp(MIN_RTO as f64, MAX_RTO as f64)
            as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pto_uses_srtt_and_rttvar() {
        assert_eq!(Congestion::pto_from(100.0, 10.0), 140);
        assert_eq!(Congestion::pto_from(1.0, 0.0), MIN_RTO);
        assert_eq!(Congestion::pto_from(90_000.0, 100.0), MAX_RTO);
    }

    #[test]
    fn pacing_refills_and_caps_at_cwnd() {
        let mut c = Congestion::new();
        c.srtt_ms = 100.0;
        c.pacing_bps = 100_000.0;
        c.tokens = 0.0;
        c.last_refill_ms = Some(0);
        c.refill(10);
        assert!((c.tokens() - 1000.0).abs() < 1.0);
        c.refill(10_000);
        assert_eq!(c.tokens(), c.cwnd_bytes);
    }

    #[test]
    fn loss_halves_once_per_pto() {
        let mut c = Congestion::new();
        let before = c.cwnd_bytes;
        c.on_retransmit(100);
        assert_eq!(c.cwnd_bytes, before / 2.0);
        c.on_retransmit(101);
        assert_eq!(c.cwnd_bytes, before / 2.0, "one decrease per PTO");
        c.on_retransmit(100 + c.pto_ms);
        assert!(c.cwnd_bytes < before / 2.0);
    }
}
