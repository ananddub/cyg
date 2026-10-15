//! Knobs for [`Plane::spawn`](super::Plane).

/// Tunables for [`Plane::spawn`](super::Plane).
#[derive(Debug, Clone, Copy)]
pub struct PlaneConfig {
    /// Per-session bounded inbox depth. A full inbox means the session task is
    /// wedged; downlink drops are expected (never block the producer).
    pub session_inbox_capacity: usize,
}

impl Default for PlaneConfig {
    fn default() -> Self {
        Self { session_inbox_capacity: 512 }
    }
}
