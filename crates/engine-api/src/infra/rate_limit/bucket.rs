use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::net::IpAddr;

use uuid::Uuid;

pub const MAX_BUCKETS: usize = 16384;

pub(super) const MAKEROOM_SCAN_BUDGET: usize = 64;

#[derive(Clone, Copy, Debug)]
pub(super) struct Bucket {
    pub(super) tokens: f64,
    pub(super) last: std::time::Instant,
}

/// One token-bucket table with its own rates and counters. `IpRateLimiter`
/// keeps two of these: per source address, and per authenticated account.
pub(super) struct Store<K: Eq + Hash + Clone> {
    pub(super) refill_per_sec: f64,
    pub(super) burst: f64,
    pub(super) buckets: HashMap<K, Bucket>,
    pub(super) order: VecDeque<K>,
    pub(super) allowed: u64,
    pub(super) denied: u64,
    pub(super) squeezed: u64,
    pub(super) swept: u64,
}

impl<K: Eq + Hash + Clone> Store<K> {
    /// A limiter that admits everything: a rate of zero means the owner turned
    /// this table off, so no caller has to special-case it.
    pub(super) fn new(refill_per_sec: f64, burst: f64) -> Store<K> {
        Store {
            refill_per_sec: if refill_per_sec > 0.0 {
                refill_per_sec.max(f64::EPSILON)
            } else {
                0.0
            },
            burst: burst.max(1.0),
            buckets: HashMap::new(),
            order: VecDeque::new(),
            allowed: 0,
            denied: 0,
            squeezed: 0,
            swept: 0,
        }
    }

    pub(super) fn enabled(&self) -> bool {
        self.refill_per_sec > 0.0
    }
}

pub struct IpRateLimiter {
    pub(super) ips: Store<IpAddr>,
    pub(super) players: Store<Uuid>,
}

impl IpRateLimiter {
    pub fn new(refill_per_sec: u32, burst: u32) -> IpRateLimiter {
        IpRateLimiter::with_rates(f64::from(refill_per_sec), f64::from(burst))
    }

    pub fn with_rates(refill_per_sec: f64, burst: f64) -> IpRateLimiter {
        IpRateLimiter {
            ips: Store::new(refill_per_sec, burst),
            players: Store::new(0.0, 1.0),
        }
    }

    /// Turns on the per-account bucket. Left off, every authenticated account
    /// is admitted, which is what a plane with no quota config asks for.
    pub fn set_player_rates(&mut self, refill_per_sec: u32, burst: u32) {
        self.players = Store::new(f64::from(refill_per_sec), f64::from(burst));
    }
}
