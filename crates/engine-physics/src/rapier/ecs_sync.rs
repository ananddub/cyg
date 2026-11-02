use bevy_ecs::prelude::*;
use engine_core::{EntityId, Position2D, Position3D, Velocity};
use rapier2d::prelude as r2d;
use rapier3d::prelude as r3d;

use super::{RapierWorld2D, RapierWorld3D};

pub fn sync_ecs_to_rapier_3d(
    query: Query<(&EntityId, &Position3D), Changed<Position3D>>,
    mut world: ResMut<RapierWorld3D>,
) {
    for (eid, pos) in &query {
        if let Some(&handle) = world.entity_handles.get(&eid.0) {
            if let Some(rb) = world.rigid_body_set.get_mut(handle) {
                rb.set_translation(r3d::Vector::new(pos.x, pos.y, pos.z), true);
            }
        }
    }
}

pub fn sync_velocity_to_rapier_3d(
    query: Query<(&EntityId, &Velocity), Changed<Velocity>>,
    mut world: ResMut<RapierWorld3D>,
) {
    for (eid, vel) in &query {
        if let Some(&handle) = world.entity_handles.get(&eid.0) {
            if let Some(rb) = world.rigid_body_set.get_mut(handle) {
                rb.set_linvel(r3d::Vector::new(vel.vx, vel.vy, vel.vz), true);
            }
        }
    }
}

pub fn step_rapier_3d(mut world: ResMut<RapierWorld3D>) {
    world.step();
}

pub fn sync_rapier_to_ecs_3d(
    mut query: Query<(&EntityId, &mut Position3D)>,
    world: Res<RapierWorld3D>,
) {
    for (eid, mut pos) in &mut query {
        if let Some(&handle) = world.entity_handles.get(&eid.0) {
            if let Some(rb) = world.rigid_body_set.get(handle) {
                let t = rb.translation();
                pos.x = t.x;
                pos.y = t.y;
                pos.z = t.z;
            }
        }
    }
}

pub fn sync_ecs_to_rapier_2d(
    query: Query<(&EntityId, &Position2D), Changed<Position2D>>,
    mut world: ResMut<RapierWorld2D>,
) {
    for (eid, pos) in &query {
        if let Some(&handle) = world.entity_handles.get(&eid.0) {
            if let Some(rb) = world.rigid_body_set.get_mut(handle) {
                rb.set_translation(r2d::Vector::new(pos.x, pos.y), true);
            }
        }
    }
}

pub fn sync_velocity_to_rapier_2d(
    query: Query<(&EntityId, &Velocity), Changed<Velocity>>,
    mut world: ResMut<RapierWorld2D>,
) {
    for (eid, vel) in &query {
        if let Some(&handle) = world.entity_handles.get(&eid.0) {
            if let Some(rb) = world.rigid_body_set.get_mut(handle) {
                rb.set_linvel(r2d::Vector::new(vel.vx, vel.vy), true);
            }
        }
    }
}

pub fn step_rapier_2d(mut world: ResMut<RapierWorld2D>) {
    world.step();
}

pub fn sync_rapier_to_ecs_2d(
    mut query: Query<(&EntityId, &mut Position2D)>,
    world: Res<RapierWorld2D>,
) {
    for (eid, mut pos) in &mut query {
        if let Some(&handle) = world.entity_handles.get(&eid.0) {
            if let Some(rb) = world.rigid_body_set.get(handle) {
                let t = rb.translation();
                pos.x = t.x;
                pos.y = t.y;
            }
        }
    }
}
