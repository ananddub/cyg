//! # Voice Jitter & Packet Loss Estimator (RFC 3550 compliant)
//!
//! Tracks in-flight packet sequence arrivals, out-of-order packets, packet loss
//! percentages, and interarrival jitter in milliseconds.

use std::time::Instant;

/// RFC 3550 Interarrival Jitter and Loss Estimator.
#[derive(Debug, Clone)]
pub struct VoiceJitterEstimator {
    /// Highest sequence number received so far.
    max_seq: u16,
    /// Base sequence number recorded upon stream start.
    base_seq: u16,
    /// Total packets received in current session.
    packets_received: u64,
    /// Rolling lost packets count.
    packets_lost: i64,
    /// Interarrival jitter in timestamp units (e.g. 48 kHz samples).
    jitter_units: f64,
    /// Last packet RTP timestamp.
    last_transit_timestamp: u32,
    /// Local arrival time of the previous packet.
    last_arrival_time: Option<Instant>,
    /// Sample rate in Hz (default: 48,000 for Opus).
    sample_rate: u32,
}

impl Default for VoiceJitterEstimator {
    fn default() -> Self {
        Self::new(48_000)
    }
}

impl VoiceJitterEstimator {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            max_seq: 0,
            base_seq: 0,
            packets_received: 0,
            packets_lost: 0,
            jitter_units: 0.0,
            last_transit_timestamp: 0,
            last_arrival_time: None,
            sample_rate,
        }
    }

    /// Record receipt of a packet with sequence number and RTP timestamp.
    pub fn record_packet(&mut self, seq: u16, timestamp: u32, now: Instant) {
        if self.packets_received == 0 {
            self.base_seq = seq;
            self.max_seq = seq;
            self.last_transit_timestamp = timestamp;
            self.last_arrival_time = Some(now);
            self.packets_received = 1;
            return;
        }

        self.packets_received += 1;

        // Sequence distance calculation handling 16-bit wraparound
        let delta = seq.wrapping_sub(self.max_seq) as i16;
        if delta > 0 {
            // New highest sequence
            let lost_between = (delta - 1).max(0) as i64;
            self.packets_lost += lost_between;
            self.max_seq = seq;
        }

        // RFC 3550 Jitter Estimation:
        // D(i, j) = (R_j - R_i) - (S_j - S_i)
        // J = J + (|D| - J) / 16
        if let Some(prev_arrival) = self.last_arrival_time {
            let arrival_delta_sec = now.duration_since(prev_arrival).as_secs_f64();
            let arrival_delta_units = arrival_delta_sec * (self.sample_rate as f64);
            let send_delta_units = timestamp.wrapping_sub(self.last_transit_timestamp) as f64;

            let diff = (arrival_delta_units - send_delta_units).abs();
            self.jitter_units += (diff - self.jitter_units) / 16.0;
        }

        self.last_transit_timestamp = timestamp;
        self.last_arrival_time = Some(now);
    }

    /// Estimated jitter in milliseconds.
    pub fn jitter_ms(&self) -> f64 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        (self.jitter_units / (self.sample_rate as f64)) * 1000.0
    }

    /// Total packets received so far.
    pub fn received_count(&self) -> u64 {
        self.packets_received
    }

    /// Total packets lost so far.
    pub fn lost_count(&self) -> i64 {
        self.packets_lost.max(0)
    }

    /// Packet loss fraction between 0.0 (0%) and 1.0 (100%).
    pub fn loss_fraction(&self) -> f64 {
        let expected = self.packets_received + (self.packets_lost.max(0) as u64);
        if expected == 0 {
            return 0.0;
        }
        (self.packets_lost.max(0) as f64) / (expected as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_jitter_estimator_continuous_stream() {
        let mut est = VoiceJitterEstimator::new(48_000);
        let start = Instant::now();

        // Feed 10 sequential packets spaced 20ms apart (960 samples @ 48kHz)
        for i in 0..10 {
            let now = start + Duration::from_millis(i * 20);
            let timestamp = (i as u32) * 960;
            est.record_packet(i as u16, timestamp, now);
        }

        assert_eq!(est.received_count(), 10);
        assert_eq!(est.lost_count(), 0);
        assert_eq!(est.loss_fraction(), 0.0);
        assert!(est.jitter_ms() < 0.1);
    }

    #[test]
    fn test_packet_loss_detection() {
        let mut est = VoiceJitterEstimator::new(48_000);
        let start = Instant::now();

        est.record_packet(0, 0, start);
        // Skip sequences 1, 2, 3 -> jump to 4
        est.record_packet(4, 4 * 960, start + Duration::from_millis(80));

        assert_eq!(est.received_count(), 2);
        assert_eq!(est.lost_count(), 3);
        assert!(est.loss_fraction() > 0.5);
    }
}
