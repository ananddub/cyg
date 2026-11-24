use crate::services::plane::Plane;

pub mod assets;
pub mod frames;
pub mod overview;
pub mod tables;
pub mod ticks;

pub use assets::{serve_asset, serve_index};
pub use overview::build_overview;
pub use tables::{build_rooms, build_rooms_scoped, build_workers};
pub use ticks::{build_clients, build_ticks};

use engine_dashboard::StatusSnapshot;

pub(crate) fn latest_sample(plane: &Plane) -> Option<StatusSnapshot> {
    plane.latest_status.load_full()
}

pub(crate) fn sample_age_ms(snap: &StatusSnapshot) -> u64 {
    (engine_bus::clock::mono_ns() / 1_000_000).saturating_sub(snap.updated_ms)
}

#[cfg(test)]
mod tests;
