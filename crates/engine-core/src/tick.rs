use crate::components::PendingDespawn;
use bevy_ecs::prelude::*;
use bevy_ecs::query::QueryState;
use bevy_ecs::schedule::Schedule;

pub struct TickRunner {
    pub world: World,
    schedule: Schedule,

    despawn_query: QueryState<(Entity, &'static PendingDespawn)>,

    scratch: Vec<Entity>,
}

impl Default for TickRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl TickRunner {

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

    pub fn add_system<M>(&mut self, system: impl IntoSystemConfigs<M>) {
        self.schedule.add_systems(system);
    }

    #[inline]
    pub fn tick(&mut self) {
        self.schedule.run(&mut self.world);
        self.cleanup_despawns();
    }

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
