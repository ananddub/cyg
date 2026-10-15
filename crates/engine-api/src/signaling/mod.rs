//! WS signaling: the browser ↔ server channel that sets up a data plane.
//!
//! One endpoint, `GET /ws/signal?seat=<seatToken>`. The seat token minted by
//! [`crate::matchmake`] **is** the authentication: an HMAC over
//! `"<room>:<expiry>"`, so a caller that never did matchmake cannot open a
//! signaling session at all.
//!
//! The hub only relays. SDP offers and ICE candidates go to the data plane
//! ([`SignalHub::take_signals`]) and answers come back through the same
//! per-seat route. No DTLS, no crypto, no peer state lives here; the seat is
//! fixed at upgrade time from the verified token, never from a message body;
//! and nothing blocks on the data plane — both directions are bounded queues
//! that drop and count rather than stalling a socket task.
//!
//! Every accepted socket costs the data plane exactly one `Close`, posted on
//! teardown. Each route records the attach generation that owns it, so a
//! reconnect supersedes the old socket and a stale teardown deletes nothing.
//!
//! Queues here are `tokio::sync::mpsc`, not the crossbeam channels in
//! [`engine_bus`]: every consumer is an async task on this plane, and a
//! `recv().await` must yield the worker thread rather than park it.

mod consts;
mod deliver;
mod frames;
mod hub;
mod replies;
mod routes;
mod socket;
mod inbound;
mod wire;

pub use consts::{INBOUND_QUEUE, PER_SEAT_QUEUE};
pub use frames::parse_ice;
pub use frames::{SignalIn, SignalOut};
pub use hub::SignalHub;
pub use replies::SeatReplies;
pub use routes::router;

#[cfg(test)]
mod tests;
