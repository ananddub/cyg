//! Seat tokens — the minting side of `CONTRACT.md`'s wire format:
//! `b64url("<room>:<exp_unix_secs>") + "." + b64url(hmac_sha256(secret, payload))`.
//! The MAC, not the map, is the authority (the raw-UDP plane holds no seat map);
//! the map only keeps seats whose `sessionId` must survive a `reconnect`.

mod b64;
mod registry;
mod token;
mod verify;

pub use b64::{b64url, b64url_decode};
pub use registry::SeatRegistry;
pub use token::{MAX_OUTSTANDING_SEATS, MAX_TTL_DRIFT_SECS, Seat};

#[cfg(test)]
mod tests;
