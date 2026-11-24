use engine_webrtc_protocol::is_newer;

use crate::plane::reliable::consts::EVENT_LIFETIME;
use crate::plane::reliable::consts::MAX_BACKOFF_SHIFT;
use crate::plane::reliable::consts::MAX_RTO;
use crate::plane::reliable::consts::MIN_RTO;
use crate::plane::reliable::lane::ReliableLane;

impl ReliableLane {
    pub fn pump(&mut self, now_ms: u64, mut send: impl FnMut(&[u8]) -> bool) {
        self.retire_expired(now_ms);
        self.congestion.refill(now_ms);
        let pto = self.congestion.pto_ms;
        let mut lost = false;
        // `make_contiguous` gives one slice so we can borrow the window and the
        // congestion state independently while still keeping strict order.
        let entries = self.window.make_contiguous();
        for entry in entries.iter_mut() {
            if entry.due > now_ms {
                break;
            }
            if !self.congestion.can_send(entry.wire.len(), now_ms) {
                break;
            }
            if !send(&entry.wire) {
                break;
            }
            entry.last_tx = now_ms;
            entry.sent_at.get_or_insert(now_ms);
            if entry.sent_once {
                entry.attempts += 1;
                self.counts.resent += 1;
                lost = true;
            } else {
                entry.sent_once = true;
                self.counts.sent += 1;
            }
            self.congestion.on_send(entry.wire.len(), now_ms);
            entry.due = now_ms + Self::backoff(pto, entry.attempts);
        }
        if lost {
            self.congestion.on_retransmit(now_ms);
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
        let mut acked_bytes: usize = 0;
        let mut newest_acked_tx: Option<u64> = None;
        let mut rtt_sample: Option<f64> = None;
        self.window.retain(|pending| {
            if covered(pending.seq) {
                released += 1;
                acked_bytes += pending.wire.len();
                newest_acked_tx =
                    Some(newest_acked_tx.map_or(pending.last_tx, |t| t.max(pending.last_tx)));
                if pending.attempts == 0 {
                    let sample = now_ms.saturating_sub(pending.last_tx) as f64;
                    if sample > 0.0 {
                        rtt_sample = Some(sample);
                    }
                }
                false
            } else {
                true
            }
        });
        if let Some(sample) = rtt_sample {
            self.congestion.on_ack(sample, now_ms, acked_bytes);
        } else if acked_bytes > 0 {
            self.congestion.on_ack(0.0, now_ms, acked_bytes);
        }
        if let Some(newest) = newest_acked_tx {
            let pending = self.window.make_contiguous();
            self.congestion.detect_rack_loss(newest, now_ms, pending);
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
