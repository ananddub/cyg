use crate::components::PendingDespawn;
use bevy_ecs::prelude::*;
use bevy_ecs::query::QueryState;
use bevy_ecs::schedule::Schedule;

/// High-performance tick runner for an isolated room.
///
/// Encapsulates an isolated `bevy_ecs::World` and `Schedule`. Runs strictly
/// single-threaded within the room context for zero lock contention and
/// maximum cache locality.
pub struct TickRunner {
    pub world: World,
    schedule: Schedule,
    /// Reused query state for the pending-despawn sweep (compiled once, not per tick).
    despawn_query: QueryState<(Entity, &'static PendingDespawn)>,
    /// Reused scratch buffer for entities to despawn this tick.
    scratch: Vec<Entity>,
}

impl Default for TickRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl TickRunner {
    /// Creates a new isolated simulation world with an empty schedule.
    pub fn new() -> Self {
        let mut world = World::new();
        let despawn_query = world.query_filtered::<(Entity, &PendingDespawn), ()>();
        Self {
            world,
            schedule: Schedule::default(),
            despawn_query,
            scratch: Vec::new(),
        }
    }

    /// Adds systems to the room tick schedule.
    pub fn add_system<M>(&mut self, system: impl IntoSystemConfigs<M>) {
        self.schedule.add_systems(system);
    }

    /// Executes one simulation tick through all scheduled systems,
    /// followed by the automatic cleanup of `PendingDespawn` entities.
    #[inline]
    pub fn tick(&mut self) {
        self.schedule.run(&mut self.world);
        self.cleanup_despawns();
    }

    /// Despawns all entities marked with `PendingDespawn`.
    fn cleanup_despawns(&mut self) {
        self.scratch.clear();
        {
            let world = &self.world;
            for (entity, _) in self.despawn_query.iter(world) {
                self.scratch.push(entity);
            }
        }
        for &entity in &self.scratch {
            self.world.despawn(entity);
        }
    }
}
