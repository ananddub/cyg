use bevy_ecs::prelude::*;
use bevy_ecs::schedule::Schedule;
use crate::components::PendingDespawn;

/// High-performance tick runner for an isolated room.
///
/// Encapsulates an isolated `bevy_ecs::World` and `Schedule`. Runs strictly
/// single-threaded within the room context for zero lock contention and
/// maximum cache locality.
pub struct TickRunner {
    pub world: World,
    schedule: Schedule,
}

impl Default for TickRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl TickRunner {
    /// Creates a new isolated simulation world with an empty schedule.
    pub fn new() -> Self {
        Self {
            world: World::new(),
            schedule: Schedule::default(),
        }
    }

    /// Adds systems to the room tick schedule.
    pub fn add_system<M>(&mut self, system: impl IntoSystemConfigs<M>) {
        self.schedule.add_systems(system);
    }

    /// Executes one simulation tick through all scheduled systems,
    /// followed by the automatic cleanup of `PendingDespawn` entities.
    pub fn tick(&mut self) {
        self.schedule.run(&mut self.world);
        self.cleanup_despawns();
    }

    /// Despawns all entities marked with `PendingDespawn`.
    fn cleanup_despawns(&mut self) {
        let mut to_despawn = Vec::new();
        {
            let mut query = self.world.query_filtered::<Entity, With<PendingDespawn>>();
            for entity in query.iter(&self.world) {
                to_despawn.push(entity);
            }
        }
        for entity in to_despawn {
            self.world.despawn(entity);
        }
    }
}
