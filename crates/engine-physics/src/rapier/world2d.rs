use bevy_ecs::prelude::*;
use rapier2d::prelude as r2d;
use std::collections::HashMap;

#[derive(Resource)]
pub struct RapierWorld2D {
    pub gravity: r2d::Vector,

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
}
