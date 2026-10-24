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
    /// Create a rigid body (0 = Dynamic, 1 = Fixed, 2/3 = Kinematic).
    pub fn create_body(&mut self, body_type: i32, x: f32, y: f32, z: f32) -> r3d::RigidBodyHandle {
        let builder = match body_type {
            1 => r3d::RigidBodyBuilder::fixed(),
            2 => r3d::RigidBodyBuilder::kinematic_velocity_based(),
            3 => r3d::RigidBodyBuilder::kinematic_position_based(),
            _ => r3d::RigidBodyBuilder::dynamic(),
        };
        let rb = builder.translation(r3d::Vector::new(x, y, z)).build();
        self.rigid_body_set.insert(rb)
    }

    /// Attach a ball collider to a rigid body (3D). Returns the collider handle.
    pub fn add_ball_collider(&mut self, handle: r3d::RigidBodyHandle, radius: f32) -> r3d::ColliderHandle {
        let collider = r3d::ColliderBuilder::ball(radius).density(1.0).build();
        self.collider_set
            .insert_with_parent(collider, handle, &mut self.rigid_body_set)
    }

    /// Attach a cuboid collider (half-extents) to a rigid body (3D).
    pub fn add_box_collider(
        &mut self,
        handle: r3d::RigidBodyHandle,
        half_x: f32,
        half_y: f32,
        half_z: f32,
    ) -> r3d::ColliderHandle {
        let collider = r3d::ColliderBuilder::cuboid(half_x, half_y, half_z)
            .density(1.0)
            .build();
        self.collider_set
            .insert_with_parent(collider, handle, &mut self.rigid_body_set)
    }

    /// Teleport a body (3D).
    pub fn set_position(&mut self, handle: r3d::RigidBodyHandle, x: f32, y: f32, z: f32) {
        if let Some(body) = self.rigid_body_set.get_mut(handle) {
            body.set_translation(r3d::Vector::new(x, y, z), true);
        }
    }

    /// Apply a linear impulse at the body's center of mass (3D).
    pub fn apply_impulse(&mut self, handle: r3d::RigidBodyHandle, ix: f32, iy: f32, iz: f32) {
        if let Some(body) = self.rigid_body_set.get_mut(handle) {
            body.apply_impulse(r3d::Vector::new(ix, iy, iz), true);
        }
    }

    /// Remove a body and all of its colliders (3D).
    pub fn remove_body(&mut self, handle: r3d::RigidBodyHandle) {
        self.rigid_body_set.remove(
            handle,
            &mut self.island_manager,
            &mut self.collider_set,
            &mut self.impulse_joint_set,
            &mut self.multibody_joint_set,
            &mut self.soft_body_set,
            true,
        );
    }

    /// True while any collider pair of the two bodies intersects (3D).
    /// Semantics are strict *overlap*: resting (touching) contacts report
    /// false — push one body slightly into the other to observe contact.
    pub fn check_intersection(
        &self,
        body_a: r3d::RigidBodyHandle,
        body_b: r3d::RigidBodyHandle,
    ) -> bool {
        let (Some(rb_a), Some(rb_b)) = (self.rigid_body_set.get(body_a), self.rigid_body_set.get(body_b)) else {
            return false;
        };
        for &col_a in rb_a.colliders() {
            for &col_b in rb_b.colliders() {
                // Strict geometric overlap (answers without any narrow-phase
                // cache), falling back to the solver's active contact state
                // for resting/touching bodies.
                if self.narrow_phase.intersection_pair(col_a, col_b) == Some(true) {
                    return true;
                }
                for pair in self.narrow_phase.contact_pairs() {
                    let matches = (pair.collider1 == col_a && pair.collider2 == col_b)
                        || (pair.collider1 == col_b && pair.collider2 == col_a);
                    if matches {
                        return pair.has_any_active_contact();
                    }
                }
            }
        }
        false
    }

    /// Drive a body like a controlled player: set its linear velocity (3D).
    pub fn set_velocity(&mut self, handle: r3d::RigidBodyHandle, vx: f32, vy: f32, vz: f32) {
        if let Some(body) = self.rigid_body_set.get_mut(handle) {
            body.set_linvel(r3d::Vector::new(vx, vy, vz), true);
        }
    }

    /// Read a body's linear velocity (3D).
    pub fn velocity(&self, handle: r3d::RigidBodyHandle) -> [f32; 3] {
        match self.rigid_body_set.get(handle) {
            Some(body) => {
                let v = body.linvel();
                [v.x, v.y, v.z]
            }
            None => [0.0; 3],
        }
    }

    /// Read a body's world position (3D).
    pub fn position(&self, handle: r3d::RigidBodyHandle) -> [f32; 3] {
        match self.rigid_body_set.get(handle) {
            Some(body) => {
                let t = body.translation();
                [t.x, t.y, t.z]
            }
            None => [0.0; 3],
        }
    }

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
            // col.position() is already the world iso in this rapier version
            // (collider rotation included); the body's iso alone drops rotated
            // colliders' rotation (see the 2D raycast for the test that
            // pinned this).
            let entity = match col.parent().and_then(|p| self.rigid_body_set.get(p)) {
                Some(_) => self
                    .handle_entities
                    .get(&col.parent().expect("checked above"))
                    .copied()
                    .unwrap_or(0),
                None => col.user_data as u32,
            };
            let world_iso = *col.position();
            let local_ray = ray.transform_by(&world_iso.inverse());
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
    /// Static arena bounds loaded from a 2D map file (`from_json`), if any.
    pub arena_width: f32,
    pub arena_height: f32,
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
            arena_width: 0.0,
            arena_height: 0.0,
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

    /// Create a rigid body:
    /// body_type: 0 = Dynamic, 1 = Fixed, 2 = KinematicVelocityBased, 3 = KinematicPositionBased
    pub fn create_body(&mut self, body_type: i32, x: f32, y: f32) -> r2d::RigidBodyHandle {
        let builder = match body_type {
            1 => r2d::RigidBodyBuilder::fixed(),
            2 => r2d::RigidBodyBuilder::kinematic_velocity_based(),
            3 => r2d::RigidBodyBuilder::kinematic_position_based(),
            _ => r2d::RigidBodyBuilder::dynamic(),
        };
        let rb = builder.translation(r2d::Vector::new(x, y)).build();
        self.rigid_body_set.insert(rb)
    }

    /// Add a circle/ball collider to a rigid body
    pub fn add_circle_collider(
        &mut self,
        body_handle: r2d::RigidBodyHandle,
        radius: f32,
        is_sensor: bool,
        restitution: f32,
        friction: f32,
    ) -> r2d::ColliderHandle {
        let col = r2d::ColliderBuilder::ball(radius)
            .sensor(is_sensor)
            .restitution(restitution)
            .friction(friction)
            .build();
        self.collider_set
            .insert_with_parent(col, body_handle, &mut self.rigid_body_set)
    }

    /// Add a box/cuboid collider to a rigid body
    pub fn add_box_collider(
        &mut self,
        body_handle: r2d::RigidBodyHandle,
        half_w: f32,
        half_h: f32,
        is_sensor: bool,
        restitution: f32,
        friction: f32,
    ) -> r2d::ColliderHandle {
        let col = r2d::ColliderBuilder::cuboid(half_w, half_h)
            .sensor(is_sensor)
            .restitution(restitution)
            .friction(friction)
            .build();
        self.collider_set
            .insert_with_parent(col, body_handle, &mut self.rigid_body_set)
    }

    /// Set linear velocity of a body
    pub fn set_velocity(&mut self, body: r2d::RigidBodyHandle, vx: f32, vy: f32) {
        if let Some(rb) = self.rigid_body_set.get_mut(body) {
            rb.set_linvel(r2d::Vector::new(vx, vy), true);
        }
    }

    /// Get linear velocity of a body
    pub fn get_velocity(&self, body: r2d::RigidBodyHandle) -> (f32, f32) {
        if let Some(rb) = self.rigid_body_set.get(body) {
            let v = rb.linvel();
            (v.x, v.y)
        } else {
            (0.0, 0.0)
        }
    }

    /// Get translation (x, y) of a body
    pub fn get_position(&self, body: r2d::RigidBodyHandle) -> Option<(f32, f32)> {
        self.rigid_body_set.get(body).map(|rb| {
            let t = rb.translation();
            (t.x, t.y)
        })
    }

    /// Set translation (x, y) of a body
    pub fn set_position(&mut self, body: r2d::RigidBodyHandle, x: f32, y: f32) {
        if let Some(rb) = self.rigid_body_set.get_mut(body) {
            rb.set_translation(r2d::Vector::new(x, y), true);
        }
    }

    /// Apply an impulse to a body
    pub fn apply_impulse(&mut self, body: r2d::RigidBodyHandle, ix: f32, iy: f32) {
        if let Some(rb) = self.rigid_body_set.get_mut(body) {
            rb.apply_impulse(r2d::Vector::new(ix, iy), true);
        }
    }

    /// Remove a body and its attached colliders
    pub fn remove_body(&mut self, body: r2d::RigidBodyHandle) {
        self.rigid_body_set.remove(
            body,
            &mut self.island_manager,
            &mut self.collider_set,
            &mut self.impulse_joint_set,
            &mut self.multibody_joint_set,
            &mut self.soft_body_set,
            true,
        );
    }

    /// Test if two bodies intersect (via any of their colliders)
    pub fn check_intersection(&self, body_a: r2d::RigidBodyHandle, body_b: r2d::RigidBodyHandle) -> bool {
        let (Some(rb_a), Some(rb_b)) = (self.rigid_body_set.get(body_a), self.rigid_body_set.get(body_b)) else {
            return false;
        };
        for &col_a in rb_a.colliders() {
            for &col_b in rb_b.colliders() {
                // Strict geometric overlap (answers without any narrow-phase
                // cache), falling back to the solver's active contact state
                // for resting/touching bodies.
                if self.narrow_phase.intersection_pair(col_a, col_b) == Some(true) {
                    return true;
                }
                for pair in self.narrow_phase.contact_pairs() {
                    let matches = (pair.collider1 == col_a && pair.collider2 == col_b)
                        || (pair.collider1 == col_b && pair.collider2 == col_a);
                    if matches {
                        return pair.has_any_active_contact();
                    }
                }
            }
        }
        false
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
            // col.position() is already the world iso in this rapier version
            // (rotation included). Reading the body's iso instead dropped the
            // collider's own rotation, so rotated map geometry cast in its
            // unrotated footprint.
            let entity = match col.parent().and_then(|p| self.rigid_body_set.get(p)) {
                Some(_) => self
                    .handle_entities
                    .get(&col.parent().expect("checked above"))
                    .copied()
                    .unwrap_or(0),
                None => col.user_data as u32,
            };
            let world_iso = *col.position();
            let local_ray = ray.transform_by(&world_iso.inverse());
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

    #[test]
    fn test_rapier2d_check_intersection() {
        let mut world = RapierWorld2D::new(0.0, 0.0);
        let coin = world.create_body(1, 400.0, 250.0);
        world.add_circle_collider(coin, 24.0, true, 0.0, 0.0);

        let player = world.create_body(0, 400.0, 250.0);
        world.add_circle_collider(player, 14.0, false, 0.0, 0.0);

        world.step();

        let hit = world.check_intersection(player, coin);
        println!("check_intersection result: {hit}");
        assert!(hit, "Player and coin at same position must intersect!");

        // Move coin to (100, 100)
        world.set_position(coin, 100.0, 100.0);
        world.step();
        let hit_far = world.check_intersection(player, coin);
        println!("after move coin far: {hit_far}");
        assert!(!hit_far, "Should not intersect when far");

        // Move player to (100, 100)
        world.set_position(player, 100.0, 100.0);
        world.step();
        let hit_moved = world.check_intersection(player, coin);
        println!("after move player to coin: {hit_moved}");
        assert!(hit_moved, "Should intersect after moving both to (100, 100)");
    }
}

/// The 2D map file — the sibling of the 3D `arena_map.json`. Boxes carry
/// `w`/`h` (full sizes) plus an optional `rotation` in radians; circles carry
/// `r`. Static geometry only: bodies are created `fixed`, so a cooked 2D map
/// loads once at boot and rooms clone it, never rebuild it per tick.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MapConfig2D {
    pub arena_width: f32,
    pub arena_height: f32,
    /// Optional gravity override `[gx, gy]`; defaults to `[0, -9.81]`.
    #[serde(default)]
    pub gravity: Option<[f32; 2]>,
    #[serde(default)]
    pub obstacles: Vec<MapObstacleDef2D>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MapObstacleDef2D {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    pub x: f32,
    pub y: f32,
    /// Box: full width. Ignored for circles.
    #[serde(default)]
    pub w: f32,
    /// Box: full height. Ignored for circles.
    #[serde(default)]
    pub h: f32,
    /// Circle radius. When present the obstacle is a circle, not a box.
    #[serde(default)]
    pub r: f32,
    /// Box rotation in radians.
    #[serde(default)]
    pub rotation: f32,
}

impl RapierWorld2D {
    /// Load a static 2D arena from JSON: fixed colliders only, one insert per
    /// obstacle, nothing per tick.
    pub fn from_json(json_str: &str) -> Result<Self, serde_json::Error> {
        let cfg: MapConfig2D = serde_json::from_str(json_str)?;
        let [gx, gy] = cfg.gravity.unwrap_or([0.0, -9.81]);
        let mut world = Self::new(gx, gy);
        world.arena_width = cfg.arena_width;
        world.arena_height = cfg.arena_height;
        for obs in cfg.obstacles {
            let handle = world.create_body(1, obs.x, obs.y);
            if obs.r > 0.0 {
                world.add_circle_collider(handle, obs.r, false, 0.0, 0.0);
            } else {
                let col = r2d::ColliderBuilder::cuboid(obs.w * 0.5, obs.h * 0.5)
                    .rotation(obs.rotation)
                    .friction(0.0)
                    .build();
                world
                    .collider_set
                    .insert_with_parent(col, handle, &mut world.rigid_body_set);
            }
        }
        Ok(world)
    }
}

#[cfg(test)]
mod map2d_tests {
    use super::*;

    /// A cooked 2D map loads as fixed colliders only: arena bounds recorded,
    /// gravity override honoured, both shapes present, and a ray at the map
    /// hits the box without any warm-up step.
    #[test]
    fn from_json_loads_a_static_2d_arena() {
        let json = r#"{
            "arena_width": 800,
            "arena_height": 500,
            "gravity": [0, -19.6],
            "obstacles": [
                { "id": 1, "name": "wall", "x": 400, "y": 250, "w": 40, "h": 200 },
                { "id": 2, "name": "rock", "x": 100, "y": 100, "r": 12 }
            ]
        }"#;
        let world = RapierWorld2D::from_json(json).expect("parse 2D map");

        assert_eq!(world.arena_width, 800.0);
        assert_eq!(world.arena_height, 500.0);
        assert_eq!(world.gravity.y, -19.6, "gravity override honoured");
        assert_eq!(world.collider_set.len(), 2, "one collider per obstacle");

        // Fixed bodies: nothing moves under the world's own gravity.
        let mut stepped = world;
        for _ in 0..30 {
            stepped.step();
        }
        let bodies: Vec<_> = stepped
            .rigid_body_set
            .iter()
            .map(|(_, b)| b.is_fixed())
            .collect();
        assert!(bodies.iter().all(|fixed| *fixed), "map geometry is static");

        // Ray straight down the +x axis at y=250 must hit the wall (x=400).
        let hit = stepped.raycast_hit([200.0, 250.0], [1.0, 0.0], 500.0);
        let (entity, toi) = hit.expect("wall is on the ray");
        assert!(toi > 150.0 && toi < 250.0, "hit before the wall plane, toi={toi}");
        assert_eq!(entity, 0, "static geometry reports entity 0");
    }

    /// Rotation is honoured: a rotated box no longer catches a ray that an
    /// axis-aligned one would.
    #[test]
    fn rotated_box_changes_the_collision_footprint() {
        let json = r#"{
            "arena_width": 400, "arena_height": 400,
            "obstacles": [
                { "id": 1, "x": 200, "y": 200, "w": 20, "h": 200, "rotation": 1.5707964 }
            ]
        }"#;
        let mut world = RapierWorld2D::from_json(json).expect("parse");
        // Rotated ~90°: local x-half 10 maps to world y, local y-half 100 to
        // world x — footprint becomes x∈[100,300], y∈[190,210] (a horizontal
        // band), the opposite orientation of the unrotated box.
        let through_band = world.raycast_hit([150.0, 300.0], [0.0, -1.0], 300.0);
        assert!(through_band.is_some(), "a vertical ray crosses the y-band");
        let above_band_short = world.raycast_hit([150.0, 300.0], [1.0, 0.0], 45.0);
        assert!(above_band_short.is_none(), "x-ray stops before the band starts at x=100");
        // A ray travelling inside the band's y-range crosses it at x=100.
        let across_band = world.raycast_hit([50.0, 200.0], [1.0, 0.0], 300.0);
        let (entity, toi) = across_band.expect("x-ray at y=200 reaches the band");
        assert_eq!(entity, 0, "static map geometry");
        assert!((toi - 50.0).abs() < 1.0, "first contact at the band edge, toi={toi}");
    }
}

