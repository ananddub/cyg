use engine_core::{EntityId, Health, Position3D};
use bevy_ecs::prelude::*;

use crate::ecs_room::state::EcsRoomState;

impl EcsRoomState {
    pub fn tick(&mut self, _dt: f64) -> Vec<u8> {
        self.runner.tick();

        let mut query = self
            .runner
            .world
            .query_filtered::<(&EntityId, &Position3D, &Health), Changed<Position3D>>();
        let mut delta = Vec::with_capacity(64);
        for (eid, pos, hp) in query.iter(&self.runner.world) {
            delta.push(0x01);
            delta.push(24);
            delta.extend_from_slice(&(eid.0 as u64).to_le_bytes());
            delta.extend_from_slice(&pos.x.to_le_bytes());
            delta.extend_from_slice(&pos.y.to_le_bytes());
            delta.extend_from_slice(&pos.z.to_le_bytes());
            delta.extend_from_slice(&hp.current.to_le_bytes());
        }
        delta
    }

    pub fn export_tlv(&mut self) -> Vec<u8> {
        let mut query = self
            .runner
            .world
            .query::<(&EntityId, &Position3D, &Health)>();
        let mut delta = Vec::with_capacity(64);
        for (eid, pos, hp) in query.iter(&self.runner.world) {
            delta.push(0x01);
            delta.push(24);
            delta.extend_from_slice(&(eid.0 as u64).to_le_bytes());
            delta.extend_from_slice(&pos.x.to_le_bytes());
            delta.extend_from_slice(&pos.y.to_le_bytes());
            delta.extend_from_slice(&pos.z.to_le_bytes());
            delta.extend_from_slice(&hp.current.to_le_bytes());
        }
        delta
    }

    pub fn raycast_hit(
        &mut self,
        origin: [f32; 3],
        dir: [f32; 3],
        max_dist: f32,
    ) -> Option<(u32, f32)> {
        self.rapier_mut().into_inner().raycast_hit(origin, dir, max_dist)
    }
}
