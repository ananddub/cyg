//! WebRTC signaling — the frame types and routing hub shared by the API
//! surface (WS endpoints) and the room host (which drains `SignalIn` and
//! answers with `SignalOut`).
//!
//! Extracted from `engine-api` so the sim crate stops depending on the entire
//! HTTP/DB/auth surface just for two enums and a hub.

mod consts;
mod deliver;
mod frames;
mod hub;
mod replies;
mod seat;

pub use consts::{INBOUND_QUEUE, PER_SEAT_QUEUE, mpsc_channel};
pub use frames::{SignalIn, SignalOut};
pub use hub::SignalHub;
pub use replies::SeatReplies;
pub use seat::Seat;
