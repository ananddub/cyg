use engine_webrtc_protocol::is_newer;

use crate::plane::reliable::consts::EVENT_LIFETIME;
use crate::plane::reliable::consts::MAX_BACKOFF_SHIFT;
use crate::plane::reliable::consts::MAX_RTO;
use crate::plane::reliable::consts::MIN_RTO;
use crate::plane::reliable::lane::ReliableLane;

impl ReliableLane {
    pub fn pump(&mut self, now_ms: u64, mut send: impl FnMut(&[u8]) -> bool) {
        self.retire_expired(now_ms);
        let rto = self.rto;
        for entry in self.window.iter_mut() {
            if entry.due > now_ms {
                break;
            }
            if !send(&entry.wire) {
                break;
            }
            entry.last_tx = now_ms;
            if entry.sent_once {
                entry.attempts += 1;
                self.counts.resent += 1;
            } else {
                entry.sent_once = true;
                self.counts.sent += 1;
            }
            entry.due = now_ms + Self::backoff(rto, entry.attempts);
        }
    }

    pub fn ack(&mut self, seq: u16, now_ms: u64) -> usize {
        self.ack_selective(seq, &[], now_ms)
    }

    pub fn ack_selective(&mut self, cum: u16, ranges: &[(u16, u16)], now_ms: u64) -> usize {
        let covered = |seq: u16| -> bool {
            if !is_newer(cum, seq) {
                return true;
            }
            ranges
                .iter()
                .any(|&(s, e)| seq.wrapping_sub(s) <= e.wrapping_sub(s))
        };
        let mut released: usize = 0;
        let mut rtt_samples: Vec<u64> = Vec::new();
        self.window.retain(|pending| {
            if covered(pending.seq) {
                released += 1;
                if pending.attempts == 0 {
                    rtt_samples.push(pending.last_tx);
                }
                false
            } else {
                true
            }
        });
        for last_tx in rtt_samples {
            let sample = now_ms.saturating_sub(last_tx) as f64;
            if sample > 0.0 {
                if self.srtt_ms == 0.0 {
                    self.srtt_ms = sample;
                } else {
                    self.srtt_ms = self.srtt_ms * 0.875 + sample * 0.125;
                }
                self.rto = Self::rto_from(self.srtt_ms);
            }
        }
        self.counts.acked += released as u64;
        released
    }

    pub(super) fn retire_expired(&mut self, now_ms: u64) {
        while let Some(front) = self.window.front() {
            if now_ms.saturating_sub(front.since) < EVENT_LIFETIME {
                break;
            }
            self.window.pop_front();
            self.counts.expired += 1;
        }
    }

    pub(super) fn backoff(rto: u64, attempts: u32) -> u64 {
        rto.saturating_mul(1u64 << attempts.min(MAX_BACKOFF_SHIFT))
    }

    pub(super) fn rto_from(srtt_ms: f64) -> u64 {
        (srtt_ms * 2.0).clamp(MIN_RTO as f64, MAX_RTO as f64) as u64
    }
}
