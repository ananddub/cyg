//! WS signaling: the browser ↔ server channel that sets up a data plane, on one
//! endpoint `GET /ws/signal?seat=<seatToken>`. The seat token minted by
//! [`crate::services::matchmake`] **is** the authentication, and the hub only
//! relays: the seat is fixed at upgrade from the verified token, never a body.
//!
//! Every accepted socket costs the data plane exactly one `Close`, posted on
//! teardown; each route records the attach generation that owns it, so a reconnect
//! supersedes the old socket and a stale teardown deletes nothing. Both directions
//! are bounded queues (tokio `mpsc`) that drop and count rather than stall a task.

mod consts;
mod deliver;
mod frames;
mod hub;
mod inbound;
mod replies;
pub(crate) mod socket;
mod wire;

pub use consts::{INBOUND_QUEUE, PER_SEAT_QUEUE};
pub use frames::{SignalIn, SignalOut};
pub use hub::SignalHub;
pub use replies::SeatReplies;

#[cfg(test)]
mod tests;
