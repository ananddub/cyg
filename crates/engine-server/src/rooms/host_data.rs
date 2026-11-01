use std::collections::HashMap;
use super::OutboundMessage;
use std::ops::{Deref, DerefMut};
use engine_physics::{PhysicsWorld3D, RapierWorld2D};
use rapier2d::prelude as r2d;
use rapier3d::prelude as r3d;

#[derive(Debug, Clone)]
pub struct TimerState {
    pub next_due: i64,
    pub interval: i64,
}

pub struct RoomHostData {
    pub physics: PhysicsWorld3D,
    pub rapier2d: RapierWorld2D,
    pub rapier2d_next_body_id: i32,
    pub rapier2d_bodies: HashMap<i32, r2d::RigidBodyHandle>,
    pub rapier2d_next_col_id: i32,
    pub rapier2d_colliders: HashMap<i32, r2d::ColliderHandle>,
    pub rapier3d: engine_physics::RapierWorld3D,
    pub rapier3d_next_body_id: i32,
    pub rapier3d_bodies: HashMap<i32, r3d::RigidBodyHandle>,
    pub rapier3d_next_col_id: i32,
    pub rapier3d_colliders: HashMap<i32, r3d::ColliderHandle>,
    pub timers: std::collections::BTreeMap<i64, TimerState>,
    pub fired_timers: Vec<i64>,
    pub navmesh: Option<std::sync::Arc<engine_nav::nav2d::NavMesh2D>>,
    pub outbox: Vec<OutboundMessage>,
    pub rng: engine_core::RoomRng,
    pub spatial: engine_spatial::SpatialIndex2D,
    pub clock_ticks: i64,
}
impl RoomHostData {

    pub fn new(physics: PhysicsWorld3D) -> Self {
        Self {
            physics,
            timers: std::collections::BTreeMap::new(),
            fired_timers: Vec::new(),
            navmesh: None,
            rapier3d: engine_physics::RapierWorld3D::new(0.0, -9.81, 0.0),
            rapier3d_next_body_id: 1,
            rapier3d_bodies: HashMap::new(),
            rapier3d_next_col_id: 1,
            rapier3d_colliders: HashMap::new(),
            rapier2d: RapierWorld2D::new(0.0, 0.0),
            rapier2d_next_body_id: 1,
            rapier2d_bodies: HashMap::new(),
            rapier2d_next_col_id: 1,
            rapier2d_colliders: HashMap::new(),
            outbox: Vec::new(),
            rng: engine_core::RoomRng::from_seed_str("unseeded"),
            spatial: engine_spatial::SpatialIndex2D::new(),
            clock_ticks: 0,
        }
    }

    pub fn seed_rng(&mut self, room_name: &str) {
        self.rng = engine_core::RoomRng::from_seed_str(room_name);
    }

}
impl RoomHostData {

    pub fn navmesh_find_path(&self, from: (f32, f32), to: (f32, f32)) -> Option<Vec<(f32, f32)>> {
        self.navmesh.as_ref().and_then(|m| m.find_path(from, to))
    }

}
impl Deref for RoomHostData {
    type Target = PhysicsWorld3D;

    fn deref(&self) -> &Self::Target {
        &self.physics
    }
}

impl DerefMut for RoomHostData {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.physics
    }
}
