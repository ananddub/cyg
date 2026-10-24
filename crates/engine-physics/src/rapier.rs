//! # Rapier 2D & 3D Physics Pipeline & ECS Integration
//!
//! Provides out-of-the-box integration for full rigid-body dynamics,
//! compound colliders, gravity, continuous collision detection (CCD),
//! and seamless single-threaded Bevy ECS synchronization.

use std::collections::HashMap;
use bevy_ecs::prelude::*;
use engine_core::{EntityId, Position2D, Position3D, Velocity};
use rapier3d::prelude as r3d;
use rapier2d::prelude as r2d;

/// High-level Rapier 3D World Wrapper with Bevy ECS resource and raycasting support
#[derive(Resource)]
pub struct RapierWorld3D {
    pub gravity: r3d::Vector,
    pub integration_parameters: r3d::IntegrationParameters,
    pub physics_pipeline: r3d::PhysicsPipeline,
    pub island_manager: r3d::IslandManager,
    pub broad_phase: r3d::BroadPhaseBvh,
    pub narrow_phase: r3d::NarrowPhase,
    pub rigid_body_set: r3d::RigidBodySet,
    pub collider_set: r3d::ColliderSet,
    pub impulse_joint_set: r3d::ImpulseJointSet,
    pub multibody_joint_set: r3d::MultibodyJointSet,
    pub soft_body_set: r3d::SoftBodySet,
    pub ccd_solver: r3d::CCDSolver,
    /// Mapping of EntityId.0 -> RigidBodyHandle
    pub entity_handles: HashMap<u32, r3d::RigidBodyHandle>,
    /// Reverse mapping of RigidBodyHandle -> EntityId.0
    pub handle_entities: HashMap<r3d::RigidBodyHandle, u32>,
}

impl Default for RapierWorld3D {
    fn default() -> Self {
        Self::new(0.0, -9.81, 0.0)
    }
}

impl RapierWorld3D {
    pub fn new(gx: f32, gy: f32, gz: f32) -> Self {
        Self {
            gravity: r3d::Vector::new(gx, gy, gz),
            integration_parameters: r3d::IntegrationParameters::default(),
            physics_pipeline: r3d::PhysicsPipeline::new(),
            island_manager: r3d::IslandManager::new(),
            broad_phase: r3d::BroadPhaseBvh::new(),
            narrow_phase: r3d::NarrowPhase::new(),
            rigid_body_set: r3d::RigidBodySet::new(),
            collider_set: r3d::ColliderSet::new(),
            impulse_joint_set: r3d::ImpulseJointSet::new(),
            multibody_joint_set: r3d::MultibodyJointSet::new(),
            soft_body_set: r3d::SoftBodySet::new(),
            ccd_solver: r3d::CCDSolver::new(),
            entity_handles: HashMap::new(),
            handle_entities: HashMap::new(),
        }
    }

    /// Step the 3D physics simulation by one tick
    pub fn step(&mut self) {
        let physics_hooks = ();
        let event_handler = ();
        self.physics_pipeline.step(
            self.gravity,
            &self.integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.rigid_body_set,
            &mut self.collider_set,
            &mut self.impulse_joint_set,
            &mut self.multibody_joint_set,
            &mut self.soft_body_set,
            &mut self.ccd_solver,
            &physics_hooks,
            &event_handler,
        );
    }

    /// Registers a mapping between an ECS EntityId and a Rapier rigid body handle.
    pub fn bind_entity(&mut self, entity_id: u32, handle: r3d::RigidBodyHandle) {
        self.entity_handles.insert(entity_id, handle);
        self.handle_entities.insert(handle, entity_id);
    }

    /// Casts a 3D ray into the physics world and returns the hit entity id and distance.
    pub fn raycast_hit(
        &mut self,
        origin: [f32; 3],
        dir: [f32; 3],
        max_dist: f32,
    ) -> Option<(u32, f32)> {
        // Direct per-collider shape cast: every collider (arena trimesh
        // included) carries its own internal BVH, so neither the broad-phase
        // tree nor a priming step is needed — a fresh all-static world
        // raycasts correctly at boot. Ray is transformed into each collider's
        // local frame; `toi` is distance along the (unnormalized) dir.
        let ray = r3d::Ray::new(
            r3d::Vector::new(origin[0], origin[1], origin[2]),
            r3d::Vector::new(dir[0], dir[1], dir[2]),
        );

        let mut best: Option<(u32, f32)> = None;
        for (_handle, col) in self.collider_set.iter() {
            // Static geometry (arena, ground) has no bound entity and
            // reports `0`; the caller distinguishes world hits from entity hits.
            let (pos, entity) = match col.parent().and_then(|p| self.rigid_body_set.get(p)) {
                Some(body) => (
                    *body.position(),
                    self.handle_entities
                        .get(&col.parent().expect("checked above"))
                        .copied()
                        .unwrap_or(0),
                ),
                None => (*col.position(), col.user_data as u32),
            };
            let local_ray = ray.transform_by(&pos.inverse());
            let Some(toi) = col.shape().cast_local_ray(&local_ray, max_dist, true) else {
                continue;
            };
            if best.as_ref().map_or(true, |(_, d)| toi < *d) {
                best = Some((entity, toi));
            }
        }
        best
    }
}

/// High-level Rapier 2D World Wrapper with Bevy ECS resource support
#[derive(Resource)]
pub struct RapierWorld2D {
    pub gravity: r2d::Vector,
    pub integration_parameters: r2d::IntegrationParameters,
    pub physics_pipeline: r2d::PhysicsPipeline,
    pub island_manager: r2d::IslandManager,
    pub broad_phase: r2d::BroadPhaseBvh,
    pub narrow_phase: r2d::NarrowPhase,
    pub rigid_body_set: r2d::RigidBodySet,
    pub collider_set: r2d::ColliderSet,
    pub impulse_joint_set: r2d::ImpulseJointSet,
    pub multibody_joint_set: r2d::MultibodyJointSet,
    pub soft_body_set: r2d::SoftBodySet,
    pub ccd_solver: r2d::CCDSolver,
    pub entity_handles: HashMap<u32, r2d::RigidBodyHandle>,
    pub handle_entities: HashMap<r2d::RigidBodyHandle, u32>,
}

impl Default for RapierWorld2D {
    fn default() -> Self {
        Self::new(0.0, -9.81)
    }
}

impl RapierWorld2D {
    pub fn new(gx: f32, gy: f32) -> Self {
        Self {
            gravity: r2d::Vector::new(gx, gy),
            integration_parameters: r2d::IntegrationParameters::default(),
            physics_pipeline: r2d::PhysicsPipeline::new(),
            island_manager: r2d::IslandManager::new(),
            broad_phase: r2d::BroadPhaseBvh::new(),
            narrow_phase: r2d::NarrowPhase::new(),
            rigid_body_set: r2d::RigidBodySet::new(),
            collider_set: r2d::ColliderSet::new(),
            impulse_joint_set: r2d::ImpulseJointSet::new(),
            multibody_joint_set: r2d::MultibodyJointSet::new(),
            soft_body_set: r2d::SoftBodySet::new(),
            ccd_solver: r2d::CCDSolver::new(),
            entity_handles: HashMap::new(),
            handle_entities: HashMap::new(),
        }
    }

    /// Step the 2D physics simulation by one tick
    pub fn step(&mut self) {
        let physics_hooks = ();
        let event_handler = ();
        self.physics_pipeline.step(
            self.gravity,
            &self.integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.rigid_body_set,
            &mut self.collider_set,
            &mut self.impulse_joint_set,
            &mut self.multibody_joint_set,
            &mut self.soft_body_set,
            &mut self.ccd_solver,
            &physics_hooks,
            &event_handler,
        );
    }

    pub fn bind_entity(&mut self, entity_id: u32, handle: r2d::RigidBodyHandle) {
        self.entity_handles.insert(entity_id, handle);
        self.handle_entities.insert(handle, entity_id);
    }

    pub fn raycast_hit(
        &mut self,
        origin: [f32; 2],
        dir: [f32; 2],
        max_dist: f32,
    ) -> Option<(u32, f32)> {
        // Direct per-collider shape cast (see the 3D variant for rationale).
        let ray = r2d::Ray::new(
            r2d::Vector::new(origin[0], origin[1]),
            r2d::Vector::new(dir[0], dir[1]),
        );

        let mut best: Option<(u32, f32)> = None;
        for (_handle, col) in self.collider_set.iter() {
            // Static geometry (arena, ground) has no bound entity and
            // reports `0`; the caller distinguishes world hits from entity hits.
            let (pos, entity) = match col.parent().and_then(|p| self.rigid_body_set.get(p)) {
                Some(body) => (
                    *body.position(),
                    self.handle_entities
                        .get(&col.parent().expect("checked above"))
                        .copied()
                        .unwrap_or(0),
                ),
                None => (*col.position(), col.user_data as u32),
            };
            let local_ray = ray.transform_by(&pos.inverse());
            let Some(toi) = col.shape().cast_local_ray(&local_ray, max_dist, true) else {
                continue;
            };
            if best.as_ref().map_or(true, |(_, d)| toi < *d) {
                best = Some((entity, toi));
            }
        }
        best
    }
}

// =========================================================================
// Bevy ECS Systems for Rapier 2D & 3D Synchronization
// =========================================================================

/// System: pushes ECS Position3D changes into Rapier rigid bodies
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

/// System: pushes ECS Velocity changes into Rapier rigid bodies. Without this
/// the velocity component is decorative — positions sync, bodies never move.
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

/// System: steps Rapier 3D simulation
pub fn step_rapier_3d(mut world: ResMut<RapierWorld3D>) {
    world.step();
}

/// System: pulls Rapier 3D rigid body positions back into ECS Position3D
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

/// System: pushes ECS Position2D changes into Rapier 2D rigid bodies
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

/// System: pushes ECS Velocity changes into Rapier 2D rigid bodies.
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

/// System: steps Rapier 2D simulation
pub fn step_rapier_2d(mut world: ResMut<RapierWorld2D>) {
    world.step();
}

/// System: pulls Rapier 2D rigid body positions back into ECS Position2D
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rapier3d_gravity_fall_and_ground_rest() {
        let mut world = RapierWorld3D::default();

        // 1. Static Ground Plane at y = 0
        let ground_collider = r3d::ColliderBuilder::cuboid(100.0, 0.5, 100.0)
            .translation(r3d::Vector::new(0.0, -0.5, 0.0))
            .build();
        world.collider_set.insert(ground_collider);

        // 2. Dynamic Falling Sphere at y = 10.0
        let sphere_rb = r3d::RigidBodyBuilder::dynamic()
            .translation(r3d::Vector::new(0.0, 10.0, 0.0))
            .build();
        let sphere_handle = world.rigid_body_set.insert(sphere_rb);
        let sphere_collider = r3d::ColliderBuilder::ball(0.5).build();
        world.collider_set.insert_with_parent(sphere_collider, sphere_handle, &mut world.rigid_body_set);

        // Step physics for 120 ticks (2 seconds) so ball fully falls to ground
        for _ in 0..120 {
            world.step();
        }

        let body = &world.rigid_body_set[sphere_handle];
        let final_y = body.translation().y;

        assert!(final_y < 1.0, "Ball must have reached ground plane");
        assert!(final_y >= 0.40, "Ball must not fall through ground plane");
    }

    #[test]
    fn test_rapier2d_simulation() {
        let mut world = RapierWorld2D::default();

        // 1. Static 2D floor at y = 0
        let floor = r2d::ColliderBuilder::cuboid(50.0, 1.0)
            .translation(r2d::Vector::new(0.0, -1.0))
            .build();
        world.collider_set.insert(floor);

        // 2. Dynamic 2D box at y = 5.0
        let box_rb = r2d::RigidBodyBuilder::dynamic()
            .translation(r2d::Vector::new(0.0, 5.0))
            .build();
        let handle = world.rigid_body_set.insert(box_rb);
        let box_col = r2d::ColliderBuilder::cuboid(0.5, 0.5).build();
        world.collider_set.insert_with_parent(box_col, handle, &mut world.rigid_body_set);

        for _ in 0..120 {
            world.step();
        }

        let y = world.rigid_body_set[handle].translation().y;
        assert!(y < 1.0);
        assert!(y >= 0.40);
    }
}
