//! # Engine Core
//!
//! Core types, room lifecycle management, fixed timestep simulation loops,
//! and session abstractions for OpenOxide Gaming.

use std::time::Duration;

/// Unique session identifier for a connected client.
pub type SessionId = String;

/// Context passed to room simulation updates on every tick.
#[derive(Debug, Clone, Copy)]
pub struct StepContext {
    pub dt: f32,
    pub dt_ms: f64,
    pub tick: u64,
}

/// Core room lifecycle trait that can be implemented natively or backed by WASM.
pub trait Room {
    /// Called when the room is initially created.
    fn on_create(&mut self);

    /// Called when a new player joins the room.
    fn on_join(&mut self, session_id: &SessionId);

    /// Called on each fixed simulation tick (e.g. 60Hz).
    fn on_tick(&mut self, ctx: StepContext);

    /// Called when an input packet is received from a client.
    fn on_input(&mut self, session_id: &SessionId, data: &[u8]);

    /// Called when a player leaves the room.
    fn on_leave(&mut self, session_id: &SessionId);

    /// Called when the room is being disposed/shut down.
    fn on_dispose(&mut self);
}

/// Monotonic clock helper for fixed timestep execution.
pub struct FixedClock {
    pub tick_rate: u32,
    pub current_tick: u64,
    pub step_duration: Duration,
}

impl FixedClock {
    pub fn new(tick_rate: u32) -> Self {
        let step_duration = Duration::from_secs_f64(1.0 / tick_rate as f64);
        Self {
            tick_rate,
            current_tick: 0,
            step_duration,
        }
    }

    pub fn advance(&mut self) -> StepContext {
        self.current_tick += 1;
        StepContext {
            dt: self.step_duration.as_secs_f32(),
            dt_ms: self.step_duration.as_secs_f64() * 1000.0,
            tick: self.current_tick,
        }
    }
}
