//! # Engine Agones
//!
//! Integration layer for Agones Dedicated Game Server orchestration.
//! Handles lifecycle transitions (Ready, Allocated, Health ping, Shutdown).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerState {
    Starting,
    Ready,
    Allocated,
    Shutdown,
}

/// Interface for interacting with the Agones sidecar.
pub trait AgonesController {
    fn mark_ready(&mut self) -> Result<(), Box<dyn std::error::Error>>;
    fn send_health_ping(&mut self) -> Result<(), Box<dyn std::error::Error>>;
    fn mark_allocated(&mut self) -> Result<(), Box<dyn std::error::Error>>;
    fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error>>;
}
