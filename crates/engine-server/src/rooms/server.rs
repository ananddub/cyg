use crate::rooms::{GameRoom, RoomHostData};
use engine_nav::nav2d::NavMesh2D;
use engine_physics::PhysicsWorld3D;
use std::collections::HashMap;

pub struct GameServer {
    pub(crate) pre: Option<engine_wasm::WasmPreInstance<RoomHostData>>,
    pub(crate) max_clients: i32,
    pub(crate) physics: PhysicsWorld3D,
    pub(crate) rooms: HashMap<std::sync::Arc<str>, GameRoom>,
    pub(crate) permissions: engine_wasm::AbiPermissions,
    pub(crate) features: engine_config::FeaturesConfig,
    pub(crate) persistent_snapshots: HashMap<String, Vec<u8>>,
    pub(crate) arena_2d: Option<engine_physics::MapConfig2D>,
    pub(crate) navmesh: Option<std::sync::Arc<NavMesh2D>>,
    pub(crate) input_coalesce: bool,
    /// Round-robin counter handing each new room its tick-stagger phase.
    pub(crate) next_room_phase: u32,
}

impl GameServer {
    pub(crate) fn new(
        pre: Option<engine_wasm::WasmPreInstance<RoomHostData>>,
        max_clients: i32,
        physics: PhysicsWorld3D,
        permissions: engine_wasm::AbiPermissions,
    ) -> Self {
        Self::new_with_features(pre, max_clients, physics, permissions, Default::default())
    }

    pub(crate) fn new_with_features(
        pre: Option<engine_wasm::WasmPreInstance<RoomHostData>>,
        max_clients: i32,
        physics: PhysicsWorld3D,
        permissions: engine_wasm::AbiPermissions,
        features: engine_config::FeaturesConfig,
    ) -> Self {
        Self {
            pre,
            max_clients,
            physics,
            rooms: HashMap::new(),
            permissions,
            features,
            persistent_snapshots: HashMap::new(),
            arena_2d: None,
            navmesh: None,
            input_coalesce: false,
            next_room_phase: 0,
        }
    }

    pub fn save_snapshot(&mut self, room_name: &str, snapshot: Vec<u8>) {
        if !self.permissions.storage {
            tracing::info!(
                "[server] 🚫 storage disabled by manifest — dropping snapshot for room '{room_name}'"
            );
            return;
        }
        tracing::info!(
            "[server] 💾 Saved persistent snapshot for room '{room_name}' ({} bytes)",
            snapshot.len()
        );
        self.persistent_snapshots
            .insert(room_name.to_string(), snapshot);
    }

    pub fn load_snapshot(&self, room_name: &str) -> Option<&Vec<u8>> {
        self.persistent_snapshots.get(room_name)
    }

    pub fn has_snapshot(&self, room_name: &str) -> bool {
        self.persistent_snapshots.contains_key(room_name)
    }

    #[inline]
    pub(crate) fn has_room_module(&self) -> bool {
        self.pre.is_some()
    }

    pub(crate) fn would_exceed_room_limit(&self, name: &str, max_rooms: u32) -> bool {
        max_rooms > 0 && !self.rooms.contains_key(name) && self.rooms.len() >= max_rooms as usize
    }

}
