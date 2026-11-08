use super::OutboundMessage;
use engine_physics::{PhysicsWorld3D, RapierWorld2D};
use rapier2d::prelude as r2d;
use rapier3d::prelude as r3d;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

#[derive(Debug, Clone)]
pub struct TimerState {
    pub next_due: i64,
    pub interval: i64,
}

pub struct RoomHostData {
    pub physics: PhysicsWorld3D,
    /// Feature snapshot this room was created with — disabled backends stay
    /// `None` for the room's whole life, so no rapier world is ever allocated.
    pub features: engine_config::FeaturesConfig,
    /// Per-room 2D rapier world; `None` when `[features] physics` excludes 2D.
    pub rapier2d: Option<RapierWorld2D>,
    pub rapier2d_next_body_id: i32,
    pub rapier2d_bodies: HashMap<i32, r2d::RigidBodyHandle>,
    pub rapier2d_next_col_id: i32,
    pub rapier2d_colliders: HashMap<i32, r2d::ColliderHandle>,
    /// Per-room 3D rapier world; `None` when `[features] physics` excludes 3D.
    pub rapier3d: Option<engine_physics::RapierWorld3D>,
    pub rapier3d_next_body_id: i32,
    pub rapier3d_bodies: HashMap<i32, r3d::RigidBodyHandle>,
    pub rapier3d_next_col_id: i32,
    pub rapier3d_colliders: HashMap<i32, r3d::ColliderHandle>,
    pub timers: std::collections::BTreeMap<i64, TimerState>,
    pub fired_timers: Vec<i64>,
    pub navmesh: Option<std::sync::Arc<engine_nav::nav2d::NavMesh2D>>,
    pub outbox: Vec<OutboundMessage>,
    /// Player ids the room kicked via Client.leave() — drained by the host
    /// after the guest call that queued them returns.
    pub kick_requests: Vec<i64>,
    pub rng: engine_core::RoomRng,
    pub spatial: engine_spatial::SpatialIndex2D,
    pub clock_ticks: i64,
}
impl RoomHostData {
    pub fn new(physics: PhysicsWorld3D) -> Self {
        Self::new_with_features(physics, &engine_config::FeaturesConfig::default())
    }

    /// Creates only the backends the operator's `[features]` table asks for.
    /// `FeaturesConfig::default()` (= both) matches the pre-config behavior.
    pub fn new_with_features(
        physics: PhysicsWorld3D,
        features: &engine_config::FeaturesConfig,
    ) -> Self {
        Self {
            physics,
            features: features.clone(),
            rapier2d: features.wants_rapier2d().then(RapierWorld2D::default),
            rapier3d: features
                .wants_rapier3d()
                .then(|| engine_physics::RapierWorld3D::new(0.0, -9.81, 0.0)),
            timers: std::collections::BTreeMap::new(),
            fired_timers: Vec::new(),
            navmesh: None,
            rapier3d_next_body_id: 1,
            rapier3d_bodies: HashMap::new(),
            rapier3d_next_col_id: 1,
            rapier3d_colliders: HashMap::new(),
            rapier2d_next_body_id: 1,
            rapier2d_bodies: HashMap::new(),
            rapier2d_next_col_id: 1,
            rapier2d_colliders: HashMap::new(),
            outbox: Vec::new(),
            kick_requests: Vec::new(),
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
