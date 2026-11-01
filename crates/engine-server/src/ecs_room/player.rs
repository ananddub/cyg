use engine_core::{EntityId, Health, PlayerControlled, Position3D, Velocity};

use crate::ecs_room::state::PLAYER_HALF_HEIGHT;
use crate::ecs_room::state::PLAYER_RADIUS;
use crate::ecs_room::state::SPAWN_HEIGHT;
use crate::ecs_room::state::EcsRoomState;
use crate::ecs_room::state::SessionInputs;

use rapier3d::prelude as r3d;

impl EcsRoomState {
    pub fn add_player(&mut self, session: &str, id: i64, x: f64, y: f64) {
        let entity_id = self.next_entity_id;
        self.next_entity_id += 1;

        let rapier = self.rapier_mut().into_inner();
        let body = r3d::RigidBodyBuilder::dynamic()
            .translation(r3d::Vector::new(x as f32, SPAWN_HEIGHT, y as f32))
            .lock_rotations()
            .build();
        let handle = rapier.rigid_body_set.insert(body);
        let collider = r3d::ColliderBuilder::capsule_y(PLAYER_HALF_HEIGHT, PLAYER_RADIUS).build();
        rapier
            .collider_set
            .insert_with_parent(collider, handle, &mut rapier.rigid_body_set);
        rapier.bind_entity(entity_id, handle);
        let _ = rapier;

        let entity = self
            .runner
            .world
            .spawn((
                EntityId(entity_id),
                Position3D {
                    x: x as f32,
                    y: SPAWN_HEIGHT,
                    z: y as f32,
                },
                Velocity::default(),
                Health::new(100.0),
                PlayerControlled {
                    client_id: id as u64,
                },
            ))
            .id();

        self.entities.insert(session.to_string(), entity);
        self.entity_ids.insert(session.to_string(), entity_id);
        self.handles.insert(entity_id, handle);
        self.runner
            .world
            .resource_mut::<SessionInputs>()
            .set(entity_id, 0);
    }

    pub fn remove_player(&mut self, session: &str) {
        if let Some(&entity) = self.entities.get(session) {
            self.runner
                .world
                .entity_mut(entity)
                .insert(engine_core::PendingDespawn);
        }
        if let Some(entity_id) = self.entity_ids.remove(session) {
            if let Some(handle) = self.handles.remove(&entity_id) {
                let rapier = self.rapier_mut().into_inner();
                rapier.rigid_body_set.remove(
                    handle,
                    &mut rapier.island_manager,
                    &mut rapier.collider_set,
                    &mut rapier.impulse_joint_set,
                    &mut rapier.multibody_joint_set,
                    &mut rapier.soft_body_set,
                    true,
                );
                rapier.entity_handles.remove(&entity_id);
            }
            self.runner
                .world
                .resource_mut::<SessionInputs>()
                .remove(entity_id);
        }
        self.entities.remove(session);
    }

    pub fn apply_input(&mut self, session: &str, data: &[u8]) {
        let Some(&entity_id) = self.entity_ids.get(session) else {
            return;
        };
        let flags = data.first().copied().unwrap_or(0);
        self.runner
            .world
            .resource_mut::<SessionInputs>()
            .set(entity_id, flags);
    }
}
