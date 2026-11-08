use std::collections::HashMap;

use bevy_ecs::prelude::*;
use engine_core::{RoomRng, TickRunner};
use engine_physics::{
    CookedMesh, RapierWorld3D, step_rapier_3d, sync_ecs_to_rapier_3d, sync_rapier_to_ecs_3d,
    sync_velocity_to_rapier_3d,
};
use rapier3d::prelude as r3d;

use crate::ecs_room::systems::apply_inputs;

pub(super) const MOVE_SPEED: f32 = 240.0;
pub(super) const SPAWN_HEIGHT: f32 = 1.0;
pub(super) const PLAYER_RADIUS: f32 = 0.4;
pub(super) const PLAYER_HALF_HEIGHT: f32 = 0.5;

#[derive(Resource)]
pub struct RoomSeed {
    pub room_name: String,
}

#[derive(Resource, Default)]
pub struct SessionInputs {
    pub(super) flags: HashMap<u32, u8>,
}

impl SessionInputs {
    pub(super) fn set(&mut self, entity_id: u32, flags: u8) {
        self.flags.insert(entity_id, flags);
    }
    pub(super) fn remove(&mut self, entity_id: u32) {
        self.flags.remove(&entity_id);
    }
}

pub struct EcsRoomState {
    pub(super) runner: TickRunner,
    pub(super) entities: HashMap<String, Entity>,
    pub(super) entity_ids: HashMap<String, u32>,
    pub(super) next_entity_id: u32,
    pub(super) handles: HashMap<u32, r3d::RigidBodyHandle>,
}

impl EcsRoomState {
    pub fn new(room_name: &str, arena: Option<&CookedMesh>) -> Self {
        let mut runner = TickRunner::new();
        let mut rapier = RapierWorld3D::default();

        let ground = r3d::ColliderBuilder::cuboid(50.0, 0.5, 50.0)
            .translation(r3d::Vector::new(0.0, -0.5, 0.0))
            .build();
        rapier.collider_set.insert(ground);

        if let Some(mesh) = arena {
            rapier.add_oxcl_mesh(mesh);
        }

        runner.world.insert_resource(rapier);
        runner
            .world
            .insert_resource(RoomRng::from_seed_str(room_name));
        runner.world.insert_resource(RoomSeed {
            room_name: room_name.to_string(),
        });
        runner.world.insert_resource(SessionInputs::default());

        runner.add_system(
            (
                apply_inputs,
                sync_ecs_to_rapier_3d,
                sync_velocity_to_rapier_3d,
                step_rapier_3d,
                sync_rapier_to_ecs_3d,
            )
                .chain(),
        );

        Self {
            runner,
            entities: HashMap::new(),
            entity_ids: HashMap::new(),
            next_entity_id: 1,
            handles: HashMap::new(),
        }
    }

    pub(super) fn rapier_mut(&mut self) -> Mut<'_, RapierWorld3D> {
        self.runner.world.resource_mut::<RapierWorld3D>()
    }

    pub fn rng(&mut self) -> Mut<'_, RoomRng> {
        self.runner.world.resource_mut::<RoomRng>()
    }

    pub fn entity_id_of(&self, session: &str) -> Option<u32> {
        self.entity_ids.get(session).copied()
    }

    pub fn player_count(&self) -> usize {
        self.entities.len()
    }
}
