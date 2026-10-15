//! `matchmake` — seat reservation, the first thing every client does.
//!
//! Same `ISeatReservation` body as the in-transport implementation plus `seatToken` (the HMAC
//! the UDP plane authenticates with) and `gamePort`. Order: bucket -> validate -> HMAC -> map write.

use auto_di::singleton;

pub mod reserve;

#[cfg(test)]
mod tests;

pub struct MatchmakeService;

#[singleton]
impl MatchmakeService {
    fn new() -> Self {
        Self
    }
}
