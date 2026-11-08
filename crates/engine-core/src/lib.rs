pub mod components;
pub mod input;
pub mod rate_limit;
pub mod rewind;
pub mod rng;
pub mod tick;

use std::time::Duration;

pub use components::*;
pub use input::{PlayerInput, PlayerInputBuffer};
pub use rate_limit::{MessageRateLimits, RateDecision};
pub use rewind::{EntityHistory, Interpolation, RewindManager, TransformSnapshot};
pub use rng::RoomRng;
pub use tick::TickRunner;

pub type SessionId = std::sync::Arc<str>;

#[derive(Debug, Clone, Copy)]
pub struct StepContext {
    pub dt: f32,
    pub dt_ms: f64,
    pub tick: u64,
}

pub trait Room {
    fn on_create(&mut self);

    fn on_join(&mut self, session_id: &SessionId);

    fn on_tick(&mut self, ctx: StepContext);

    fn on_input(&mut self, session_id: &SessionId, input: PlayerInput);

    fn on_leave(&mut self, session_id: &SessionId);

    fn on_dispose(&mut self);
}

pub struct FixedClock {
    pub tick_rate: u32,
    pub current_tick: u64,
    pub step_duration: Duration,
}

impl FixedClock {
    pub fn new(tick_rate: u32) -> Self {
        assert!(
            tick_rate > 0,
            "FixedClock::new: tick_rate must be > 0 (got 0)"
        );
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
