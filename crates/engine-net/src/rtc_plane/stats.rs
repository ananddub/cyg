use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// How often the demux speaks up about a full session inbox. One warn per
/// thousand dropped packets keeps a sustained backlog visible in the logs
/// without paying a log line per packet.
const WARN_EVERY_N_DROPS: u64 = 1000;

/// Backpressure counters for the rtc-plane demux. The session inboxes are
/// bounded (`session_inbox_capacity`, 512 by default); when one is full the
/// demux drops the packet instead of blocking the UDP loop, and these
/// counters turn that silent loss into something observable.
#[derive(Clone, Default)]
pub struct DemuxStats {
    dropped_inbound: Arc<AtomicU64>,
}

impl DemuxStats {
    pub fn new() -> DemuxStats {
        DemuxStats::default()
    }

    /// Records one packet dropped because its session inbox was full. The
    /// atomic add is the whole per-packet cost; the warn is amortized.
    pub(crate) fn record_inbound_drop(&self, session_id: &str) {
        let total = self.dropped_inbound.fetch_add(1, Ordering::Relaxed) + 1;
        if total % WARN_EVERY_N_DROPS == 1 {
            tracing::warn!(
                session = session_id,
                dropped = total,
                "rtc session inbox full; dropping inbound packets (one warn per {WARN_EVERY_N_DROPS} drops)"
            );
        }
    }

    /// Total packets dropped on the demux's `try_send` path since the plane
    /// started, across every session.
    pub fn dropped_inbound(&self) -> u64 {
        self.dropped_inbound.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_are_counted_across_clones() {
        let stats = DemuxStats::new();
        let clone = stats.clone();
        assert_eq!(stats.dropped_inbound(), 0);
        for _ in 0..2500 {
            clone.record_inbound_drop("sess-1");
        }
        assert_eq!(stats.dropped_inbound(), 2500);
    }
}
