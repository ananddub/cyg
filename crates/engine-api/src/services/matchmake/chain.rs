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
    /// (live occupancy + fresh outstanding) is below the cap. The status
    /// snapshot reports one row per hosted room keyed by the full room key
    /// (`<variant>__<filters>`, or `...__gN` for chained generations) — the
    /// same key this picker queries — so occupancy is matched exactly per
    /// generation key. Prefix matching would double-count: `base__g2` is also
    /// a prefix of `base__g20`.
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

    /// `create`: pick the first generation with no live occupancy and no
    /// outstanding fresh seats, so the new player gets a fresh room.
    pub fn pick_create_generation(
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
            let live = occupancy(&key) as usize;
            let fresh = self.outstanding(&key, now);
            if live == 0 && fresh == 0 {
                return generation;
            }
            generation += 1;
            if generation > 512 {
                return generation;
            }
        }
    }

    /// `join`: pick the first generation that exists (has live or fresh seats)
    /// and has space. Returns None if no joinable room exists.
    pub fn pick_join_generation(
        &mut self,
        base_key: &str,
        cap: i32,
        occupancy: impl Fn(&str) -> u32,
        now: Instant,
    ) -> Option<u32> {
        if cap <= 0 {
            return None;
        }
        let mut generation = 0u32;
        let mut saw_any = false;
        loop {
            let key = if generation == 0 {
                base_key.to_string()
            } else {
                format!("{base_key}__g{generation}")
            };
            let live = occupancy(&key) as usize;
            let fresh = self.outstanding(&key, now);
            if live > 0 || fresh > 0 {
                saw_any = true;
                if (live as i64 + fresh as i64) < cap as i64 {
                    return Some(generation);
                }
            } else if saw_any {
                // We passed existing generations and hit an empty gap; no room to join.
                return None;
            }
            generation += 1;
            if generation > 512 {
                return if saw_any { Some(generation) } else { None };
            }
            // If we've never seen any room and we're past gen0, there's nothing to join.
            if generation > 1 && !saw_any {
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_seats_reserve_capacity_until_their_ttl_lapses() {
        let mut chain = ChainTracker::default();
        let now = Instant::now();
        chain.record("duo__mumbai", now);
        chain.record("duo__mumbai", now);
        assert_eq!(chain.pick_generation("duo__mumbai", 2, |_| 0, now), 1);
        assert_eq!(
            chain.pick_generation("duo__mumbai", 2, |_| 0, now + OUTSTANDING_TTL),
            0,
            "expired reservations no longer hold the slot"
        );
    }

    #[test]
    fn live_occupancy_and_fresh_seats_share_the_cap() {
        let mut chain = ChainTracker::default();
        let now = Instant::now();
        chain.record("duo__mumbai", now);
        let occupancy = |key: &str| if key == "duo__mumbai" { 1 } else { 0 };
        assert_eq!(chain.pick_generation("duo__mumbai", 2, occupancy, now), 1);
        assert_eq!(
            chain.pick_generation("duo__mumbai", 3, occupancy, now),
            0,
            "one live + one fresh leaves a third slot"
        );
    }
}
