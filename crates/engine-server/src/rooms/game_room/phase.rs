use crate::rooms::*;

impl GameRoom {
    #[inline]
    pub fn is_hibernating(&self) -> bool {
        self.sessions.is_empty() && self.spectators.is_empty()
    }

    #[inline]
    pub fn phase(&self) -> engine_bus::RoomPhase {
        self.lifecycle.phase
    }

    pub(crate) fn note_state(&mut self) {
        if self.lifecycle.phase == engine_bus::RoomPhase::Closed {
            return;
        }
        let next = if !self.is_hibernating() {
            engine_bus::RoomPhase::Active
        } else if self.lifecycle.phase == engine_bus::RoomPhase::Created {
            return;
        } else {
            engine_bus::RoomPhase::Hibernating
        };
        self.set_state(next);
    }

    pub(crate) fn set_state(&mut self, next: engine_bus::RoomPhase) -> bool {
        if self.lifecycle.phase == next {
            return false;
        }
        if !self.lifecycle.phase.allows(next) {
            tracing::warn!("[server] room refused {:?} -> {next:?}", self.lifecycle.phase);
            return false;
        }
        self.lifecycle.phase = next;
        true
    }

    #[inline]
    pub fn is_relay(&self) -> bool {
        self.backend == RoomBackend::Relay
    }

    #[inline]
    pub fn is_wasm(&self) -> bool {
        self.backend == RoomBackend::Wasm
    }

    #[inline]
    pub fn is_ecs(&self) -> bool {
        self.backend == RoomBackend::Ecs
    }

    pub(crate) fn tick_ecs(&mut self, dt: f64) -> usize {
        let Some(ref mut ecs) = self.ecs else {
            return 0;
        };
        let delta = ecs.tick(dt);
        if delta.is_empty() {
            return 0;
        }
        const MAX_ECS_OUTBOX: usize = 256;
        static DELTA_EVENT: std::sync::LazyLock<std::sync::Arc<str>> =
            std::sync::LazyLock::new(|| std::sync::Arc::from("delta"));
        let len = delta.len();
        if self.ecs_outbox.len() < MAX_ECS_OUTBOX {
            self.ecs_outbox.push(OutboundMessage {
                target: 0,
                event: DELTA_EVENT.clone(),
                payload: bytes::Bytes::from(delta),
            });
        }
        len
    }

    pub(crate) fn ecs_input(&mut self, session: &str, data: &[u8]) {
        if let Some(ref mut ecs) = self.ecs {
            ecs.apply_input(session, data);
        }
    }
}
