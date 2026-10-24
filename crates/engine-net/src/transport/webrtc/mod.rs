//! # WebRTC Server Transport (Sans-IO via str0m)
//!
//! Direct Browser-to-Server WebRTC DataChannels over UDP (Star Topology):
//! - `game-unreliable`: Unordered, 0-retransmits high-frequency channel for 50Hz–128Hz input & state deltas.
//! - `game-reliable`: Ordered reliable channel for critical game events (fire, damage, kills).
//!
//! Uses `str0m` with zero async runtime overhead, integrating directly into `mio` event loops.
//!
//! ## The channel contract, and why it is not guessed
//!
//! A label is the only thing that names a lane, so the mapping is exact:
//! [`contract::classify_label`] accepts `game-unreliable` and `game-reliable`
//! verbatim, tolerates a documented legacy alias with a one-time warning, and
//! ignores everything else. An unknown channel is never folded into the reliable
//! lane — that would hand a debug or telemetry socket the reliability guarantee
//! gameplay events depend on. Because a label can lie, the SCTP parameters the
//! peer actually negotiated are checked against the lane too, and the mismatch is
//! reported in [`WebRtcServer::stats`] rather than papered over.
//!
//! Reliability is explicit in both directions: a delta body is stamped and split
//! to the path MTU before it leaves ([`WebRtcServer::send_delta`]), and an inbound
//! frame is only honoured on the lane whose reliability makes it meaningful — a
//! 4-byte ack pattern that arrived over a 0-retransmit channel is not proof of
//! delivery, so it is refused and counted instead of believed.
//!
//! ## Layout
//!
//! - `contract` — lanes, labels, frame classification, fragmentation and queues
//! - `types` — the event, counter and report shapes the host reads
//! - `session` — one peer's `str0m::Rtc` plus its lanes and lane queues
//! - `server` — session table, UDP ingress, tick and the output drains
//! - `signaling` — SDP offer/answer and trickle ICE
//! - `send` — deltas and framed messages leaving the transport
//! - `write` — the write-or-queue decision per frame
//! - `events` — turning `str0m` output into events, inbound frames and counters
//! - `status` — read-only per-client and per-transport queries

pub mod contract;
pub mod game_transport;

mod events;
mod send;
mod server;
mod session;
mod signaling;
mod status;
mod types;
mod write;

#[cfg(test)]
mod tests;

pub use game_transport::WebRtcTransport;
pub use server::WebRtcServer;
pub use types::{DeltaSendReport, TransportStats, WebRtcServerEvent};
