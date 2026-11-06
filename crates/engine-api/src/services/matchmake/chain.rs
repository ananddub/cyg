//! Full-room chaining: when a variant room reaches its cap, the next join
//! lands in a fresh generation of the same variant (`...__g2`, `__g3`…).
//! The picker combines the host's live occupancy (status snapshot, 1 Hz)
//! with seats minted-but-not-yet-connected in the last few seconds, so a
//! burst of joins inside one status interval cannot overrun the cap.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Seats minted recently per room key — subtracted nothing, added to the
/// snapshot's occupancy while fresh. Entries age out, so an abandoned seat
/// stops reserving capacity.
const OUTSTANDING_TTL: Duration = Duration::from_secs(15);

#[derive(Default)]
pub struct ChainTracker {
    outstanding: HashMap<String, Vec<Instant>>,
}

impl ChainTracker {
    /// Record one seat minted for `room_key`.
    pub fn record(&mut self, room_key: &str, now: Instant) {
        let list = self.outstanding.entry(room_key.to_string()).or_default();
        list.push(now);
    }

    /// Fresh outstanding seats for `room_key`.
    pub fn outstanding(&mut self, room_key: &str, now: Instant) -> usize {
        let Some(list) = self.outstanding.get_mut(room_key) else {
            return 0;
        };
        list.retain(|at| now.duration_since(*at) < OUTSTANDING_TTL);
        list.len()
    }

    /// Pick the generation for a variant join: the smallest `g` whose total
    /// (live occupancy + fresh outstanding) is below the cap. Occupancy is
    /// read from the snapshot by prefix — every generation of the variant
    /// shares the `base` prefix, generations are numbered from g1 upward.
    pub fn pick_generation(
        &mut self,
        base_key: &str,
        cap: i32,
        occupancy: impl Fn(&str) -> u32,
        now: Instant,
    ) -> u32 {
        if cap <= 0 {
            return 0;
        }
        let mut generation = 0u32;
        loop {
            let key = if generation == 0 {
                base_key.to_string()
            } else {
                format!("{base_key}__g{generation}")
            };
            let live = occupancy(&key);
            let fresh = self.outstanding(&key, now);
            if (live as i64 + fresh as i64) < cap as i64 {
                return generation;
            }
            generation += 1;
            if generation > 512 {
                return generation; // absurd guard; the cap is operator-set
            }
        }
    }
}
