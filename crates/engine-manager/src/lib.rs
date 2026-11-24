//! Central lifecycle and runtime state management for game servers and rooms.
//!
//! This crate owns the authoritative lifecycle/state primitives that higher
//! layers (host loop, room runtime, control plane) drive. Keeping them here
//! prevents lifecycle rules and state transitions from being scattered across
//! unrelated modules.

pub mod room_lifecycle;
pub mod server_lifecycle;
pub mod threads;

pub use room_lifecycle::{CloseReason, DRAIN_REASON, RoomLifecycle, lifecycle_close_code};
pub use server_lifecycle::{DrainState, ServerLifecycle, ServerManager, ServerPhase};
pub use threads::{ThreadRegistry, ThreadRole, ThreadState, ThreadStatus};
