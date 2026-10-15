
pub mod assets;
pub mod frames;
pub mod overview;
pub mod tables;
pub mod ticks;

pub use assets::{serve_asset, serve_index};
pub use overview::build_overview;
pub use tables::{build_rooms, build_workers};
pub use ticks::{build_clients, build_ticks};

use engine_dashboard::StatusSnapshot;

use crate::ApiState;

pub(crate) fn latest_sample(state: &ApiState) -> Option<StatusSnapshot> {
    state.latest_status.lock().ok().and_then(|g| g.clone())
}

pub(crate) fn sample_age_ms(snap: &StatusSnapshot) -> u64 {
    (engine_bus::clock::mono_ns() / 1_000_000).saturating_sub(snap.updated_ms)
}

#[cfg(test)]
mod tests;
