use bevy_ecs::prelude::*;
use rapier3d::prelude as r3d;
use std::collections::HashMap;

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

    pub entity_handles: HashMap<u32, r3d::RigidBodyHandle>,

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

    pub fn add_ball_collider(
        &mut self,
        handle: r3d::RigidBodyHandle,
        radius: f32,
    ) -> r3d::ColliderHandle {
        let collider = r3d::ColliderBuilder::ball(radius).density(1.0).build();
        self.collider_set
            .insert_with_parent(collider, handle, &mut self.rigid_body_set)
    }

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

    pub fn set_position(&mut self, handle: r3d::RigidBodyHandle, x: f32, y: f32, z: f32) {
        if let Some(body) = self.rigid_body_set.get_mut(handle) {
            body.set_translation(r3d::Vector::new(x, y, z), true);
        }
    }

    pub fn apply_impulse(&mut self, handle: r3d::RigidBodyHandle, ix: f32, iy: f32, iz: f32) {
        if let Some(body) = self.rigid_body_set.get_mut(handle) {
            body.apply_impulse(r3d::Vector::new(ix, iy, iz), true);
        }
    }

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

    pub fn bind_entity(&mut self, entity_id: u32, handle: r3d::RigidBodyHandle) {
        self.entity_handles.insert(entity_id, handle);
        self.handle_entities.insert(handle, entity_id);
    }
}
