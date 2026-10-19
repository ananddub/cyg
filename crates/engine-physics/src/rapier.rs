//! # Rapier 2D & 3D Physics Pipeline
//!
//! Provides out-of-the-box integration for full rigid-body dynamics,
//! compound colliders, gravity, and continuous collision detection (CCD).
//!
//! The default Rapier features used here are fixed-step but do **not** claim
//! cross-platform bit-exact determinism. If you need lockstep/replay across
//! operating systems, enable and validate Rapier's `enhanced-determinism`
//! feature (or use the fixed-point helpers) before relying on checksum
//! equality. Server-authoritative simulation does not require cross-platform
//! determinism.

use rapier3d::prelude as r3d;
use rapier2d::prelude as r2d;

/// High-level Rapier 3D World Wrapper
pub struct RapierWorld3D {
    pub gravity: r3d::Vector,
    pub integration_parameters: r3d::IntegrationParameters,
    pub physics_pipeline: r3d::PhysicsPipeline,
    pub island_manager: r3d::IslandManager,
    pub broad_phase: r3d::DefaultBroadPhase,
    pub narrow_phase: r3d::NarrowPhase,
    pub rigid_body_set: r3d::RigidBodySet,
    pub collider_set: r3d::ColliderSet,
    pub impulse_joint_set: r3d::ImpulseJointSet,
    pub multibody_joint_set: r3d::MultibodyJointSet,
    pub soft_body_set: r3d::SoftBodySet,
    pub ccd_solver: r3d::CCDSolver,
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
            broad_phase: r3d::DefaultBroadPhase::new(),
            narrow_phase: r3d::NarrowPhase::new(),
            rigid_body_set: r3d::RigidBodySet::new(),
            collider_set: r3d::ColliderSet::new(),
            impulse_joint_set: r3d::ImpulseJointSet::new(),
            multibody_joint_set: r3d::MultibodyJointSet::new(),
            soft_body_set: r3d::SoftBodySet::new(),
            ccd_solver: r3d::CCDSolver::new(),
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
}

/// High-level Rapier 2D World Wrapper
pub struct RapierWorld2D {
    pub gravity: r2d::Vector,
    pub integration_parameters: r2d::IntegrationParameters,
    pub physics_pipeline: r2d::PhysicsPipeline,
    pub island_manager: r2d::IslandManager,
    pub broad_phase: r2d::DefaultBroadPhase,
    pub narrow_phase: r2d::NarrowPhase,
    pub rigid_body_set: r2d::RigidBodySet,
    pub collider_set: r2d::ColliderSet,
    pub impulse_joint_set: r2d::ImpulseJointSet,
    pub multibody_joint_set: r2d::MultibodyJointSet,
    pub soft_body_set: r2d::SoftBodySet,
    pub ccd_solver: r2d::CCDSolver,
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
            broad_phase: r2d::DefaultBroadPhase::new(),
            narrow_phase: r2d::NarrowPhase::new(),
            rigid_body_set: r2d::RigidBodySet::new(),
            collider_set: r2d::ColliderSet::new(),
            impulse_joint_set: r2d::ImpulseJointSet::new(),
            multibody_joint_set: r2d::MultibodyJointSet::new(),
            soft_body_set: r2d::SoftBodySet::new(),
            ccd_solver: r2d::CCDSolver::new(),
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

        // Ball should fall from y = 10 and rest on ground (y ~ 0.5 with radius 0.5)
        println!("[Rapier 3D] Ball initial y=10.0 -> rested at y={:.4}", final_y);
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
        println!("[Rapier 2D] Box rested at y={:.4}", y);
        assert!(y < 1.0);
        assert!(y >= 0.40);
    }
}
