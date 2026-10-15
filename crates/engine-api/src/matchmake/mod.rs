//! `matchmake` — seat reservation, the first thing every client does.
//!
//! Same `ISeatReservation` JSON body as the in-transport implementation, so an
//! existing page works against either port, plus two fields a client needs to
//! get onto the data planes: `seatToken` (the HMAC token the raw-UDP plane
//! authenticates with) and `gamePort` (where the game WebSocket lives, which is
//! not this port).
//!
//! The ordering that matters:
//!
//! ```text
//! charge the per-IP bucket  ->  validate method/room  ->  HMAC work  ->  map write
//! ```
//!
//! A seat's room comes from the *signature*, never from the URL: on
//! `reconnect`/`rejoin` the URL's room is validated for shape but the resumed
//! seat keeps the room it was minted for.

mod reserve;
mod routes;
mod wire;

pub use reserve::reserve;
pub use routes::{MATCHMAKE_METHODS, MAX_BODY_BYTES, router, valid_room_name};
pub use wire::{SeatResponse, json_string_field};

#[cfg(test)]
mod tests;
