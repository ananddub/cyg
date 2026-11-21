use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::net::IpAddr;
use std::sync::{Mutex, MutexGuard, PoisonError};

use uuid::Uuid;

/// Total ceiling on per-source buckets across the whole limiter: the flood
/// guard against spoofed-address memory exhaustion.
pub const MAX_BUCKETS: usize = 16384;

/// How many independent per-source tables the limiter keeps. Each shard has
/// its own lock and its own token-bucket table, so two matchmake requests
/// arriving from different addresses never contend on one mutex. Sharding
/// only spreads the locks; the per-IP token-bucket math lives in `charge`
/// unchanged, and every shard applies the same rates.
pub const SHARDS: usize = 16;

/// Each shard holds a slice of the global ceiling, so a fully flooded
/// limiter still tracks at most `MAX_BUCKETS` sources in total.
pub(super) const MAX_BUCKETS_PER_SHARD: usize = MAX_BUCKETS / SHARDS;

pub(super) const MAKEROOM_SCAN_BUDGET: usize = 64;

/// Password routes (Argon2 per attempt) draw from their own, much tighter
/// per-address bucket. Fixed rates: the cost of an attempt is set by the
/// hash, not by the operator's traffic profile, so these do not scale with
/// `rate_limit_rps`.
pub const AUTH_REFILL_PER_SEC: f64 = 0.5;
pub const AUTH_BURST: f64 = 10.0;

#[derive(Clone, Copy, Debug)]
pub(super) struct Bucket {
    pub(super) tokens: f64,
    pub(super) last: std::time::Instant,
}

/// One token-bucket table with its own rates and counters. `IpRateLimiter`
/// keeps `SHARDS` of these for source addresses, and one for authenticated
/// accounts.
pub(super) struct Store<K: Eq + Hash + Clone> {
    pub(super) refill_per_sec: f64,
    pub(super) burst: f64,
    pub(super) max_buckets: usize,
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
        Store::with_ceiling(refill_per_sec, burst, MAX_BUCKETS)
    }

    pub(super) fn with_ceiling(
        refill_per_sec: f64,
        burst: f64,
        max_buckets: usize,
    ) -> Store<K> {
        Store {
            refill_per_sec: if refill_per_sec > 0.0 {
                refill_per_sec.max(f64::EPSILON)
            } else {
                0.0
            },
            burst: burst.max(1.0),
            max_buckets,
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

/// FNV-1a over 16 bytes picks the shard: byte-at-a-time, no allocation, and
/// consecutive keys spread evenly across the table.
fn fnv_shard(octets: &[u8; 16]) -> usize {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = FNV_OFFSET;
    for &byte in octets.iter() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    (hash % SHARDS as u64) as usize
}

pub(super) fn shard_of(addr: &IpAddr) -> usize {
    let octets: [u8; 16] = match addr {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            let mut out = [0u8; 16];
            out[..4].copy_from_slice(&o);
            out
        }
        IpAddr::V6(v6) => v6.octets(),
    };
    fnv_shard(&octets)
}

/// Uuid keys (authenticated accounts) shard with the same FNV spread as
/// addresses — account traffic is the other contended table.
pub(super) fn shard_of_uuid(uid: &Uuid) -> usize {
    fnv_shard(uid.as_bytes())
}

/// A poisoned shard still holds valid token buckets — a panic elsewhere in
/// the process must not take the plane's traffic down with it, which is the
/// same deal the old single `Mutex` call sites had.
pub(super) fn lock<'a, K: Eq + Hash + Clone>(
    shard: &'a Mutex<Store<K>>,
) -> MutexGuard<'a, Store<K>> {
    shard.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct IpRateLimiter {
    pub(super) ips: [Mutex<Store<IpAddr>>; SHARDS],
    /// Per-account buckets, sharded like the address tables: every
    /// authenticated endpoint charges through here, so a single mutex would
    /// be the one place left where unrelated requests contend.
    pub(super) players: [Mutex<Store<Uuid>>; SHARDS],
    /// The tighter per-address table for password routes; keyed by address
    /// like `ips`, but always on and refilled an order of magnitude slower.
    pub(super) auth: [Mutex<Store<IpAddr>>; SHARDS],
}

impl IpRateLimiter {
    pub fn new(refill_per_sec: u32, burst: u32) -> IpRateLimiter {
        IpRateLimiter::with_rates(f64::from(refill_per_sec), f64::from(burst))
    }

    pub fn with_rates(refill_per_sec: f64, burst: f64) -> IpRateLimiter {
        IpRateLimiter {
            ips: std::array::from_fn(|_| {
                Mutex::new(Store::with_ceiling(
                    refill_per_sec,
                    burst,
                    MAX_BUCKETS_PER_SHARD,
                ))
            }),
            players: std::array::from_fn(|_| Mutex::new(Store::new(0.0, 1.0))),
            auth: std::array::from_fn(|_| {
                Mutex::new(Store::with_ceiling(
                    AUTH_REFILL_PER_SEC,
                    AUTH_BURST,
                    MAX_BUCKETS_PER_SHARD,
                ))
            }),
        }
    }

    /// Turns on the per-account bucket. Left off, every authenticated account
    /// is admitted, which is what a plane with no quota config asks for.
    pub fn set_player_rates(&self, refill_per_sec: u32, burst: u32) {
        for shard in &self.players {
            *lock(shard) = Store::new(f64::from(refill_per_sec), f64::from(burst));
        }
    }

    pub(super) fn shard(&self, addr: &IpAddr) -> &Mutex<Store<IpAddr>> {
        &self.ips[shard_of(addr)]
    }

    /// Queue entries across every shard: the rotation tracks the map, and the
    /// sum of the two stays equal.
    #[cfg(test)]
    pub(super) fn order_len(&self) -> usize {
        self.ips
            .iter()
            .map(|shard| lock(shard).order.len())
            .sum()
    }
}
