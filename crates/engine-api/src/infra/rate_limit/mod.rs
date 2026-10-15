//! Per-source token bucket for matchmake, the one endpoint an unauthenticated
//! client can hammer. Only the socket address is charged — an `X-Forwarded-For`
//! header would let a caller pick which bucket it bypasses. At most [`MAX_BUCKETS`]
//! sources are tracked and making room is a bounded rotation, never an O(n) sweep.

mod admit;
mod bucket;
mod read;

pub use bucket::{IpRateLimiter, MAX_BUCKETS};

#[cfg(test)]
mod tests;
