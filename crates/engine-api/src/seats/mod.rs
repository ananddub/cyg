//! Seat tokens — the minting side of the wire format in `CONTRACT.md`.
//!
//! ```text
//! token = b64url("<room>:<exp_unix_secs>") + "." + b64url(hmac_sha256(secret, payload))
//! ```
//!
//! The MAC, not the map, is the authority: `verify` accepts any unexpired token
//! carrying this secret, even one minted by another worker, because the raw-UDP
//! plane holds no seat map at all. The map only carries seats whose `sessionId`
//! must survive a `reconnect`.

mod b64;
mod registry;
mod token;
mod verify;

pub use b64::{b64url, b64url_decode};
pub use registry::SeatRegistry;
pub use token::{MAX_OUTSTANDING_SEATS, MAX_TTL_DRIFT_SECS, Seat};

#[cfg(test)]
mod tests;
