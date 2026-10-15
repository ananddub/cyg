//! Status: the HTTP telemetry routes (`/status`, `/api/rooms`) and the queue pump behind them.
//!
//! The sim plane publishes a `StatusSnapshot` about once a second onto `bus.status_tx`;
//! exactly one task ([`crate::infra::pump`]) drains it so `/status` never races a reader.

use auto_di::singleton;

pub mod rooms;
pub mod snapshot;

#[cfg(test)]
mod tests;

pub struct StatusService;

#[singleton]
impl StatusService {
    fn new() -> Self {
        Self
    }
}
