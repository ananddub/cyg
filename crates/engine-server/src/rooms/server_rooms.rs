use crate::rooms::*;

impl GameServer {
    pub(crate) fn get_or_create(
        &mut self,
        name: &str,
        create_options: &[u8],
    ) -> Result<&mut GameRoom, engine_wasm::WasmError> {
        if !self.rooms.contains_key(name) {
            let mut room = if let Some(pre) = &self.pre {
                let mut room = GameRoom::new_with_extras(
                    pre,
                    self.max_clients,
                    self.physics.clone(),
                    self.arena_2d.as_ref(),
                    self.navmesh.clone(),
                    create_options,
                    &self.features,
                )?;
                if let Some(w) = room.wasm.as_mut() {
                    w.data_mut().seed_rng(name);
                }
                tracing::info!(
                    "[server] room '{name}' created with 2D/3D physics world (ABI v{})",
                    engine_wasm::CURRENT_ABI_VERSION
                );
                room
            } else {
                tracing::info!(
                    "[server] room '{name}' created in relay mode (no room module linked)"
                );
                GameRoom::new_relay()
            };
            room.storage_enabled = self.permissions.storage;
            room.input_coalesce = self.input_coalesce;
            room.tick_phase = self.next_room_phase;
            self.next_room_phase = self.next_room_phase.wrapping_add(1);

            if let Some(snap_bytes) = self.persistent_snapshots.get(name) {
                if let Err(e) = room.import_snapshot(snap_bytes) {
                    tracing::error!(
                        "[server] ⚠️ Failed to restore persistent snapshot for '{name}': {e}"
                    );
                } else {
                    tracing::info!("[server] 🔄 Restored persistent snapshot for room '{name}'");
                }
            }

            self.rooms.insert(std::rc::Rc::from(name), room);
        }
        Ok(self.rooms.get_mut(name).expect("room just inserted"))
    }

    pub(crate) fn get_or_create_ecs(
        &mut self,
        name: &str,
        arena: Option<&engine_physics::oxcl::CookedMesh>,
    ) -> &mut GameRoom {
        if !self.rooms.contains_key(name) {
            let mut room = GameRoom::new_ecs(name, arena);
            tracing::info!(
                "[server] room '{name}' created with native bevy_ecs backend (Rapier3D + RoomRng{})",
                if arena.is_some() { " + OXCL arena" } else { "" }
            );
            room.storage_enabled = self.permissions.storage;
            room.tick_phase = self.next_room_phase;
            self.next_room_phase = self.next_room_phase.wrapping_add(1);
            self.rooms.insert(std::rc::Rc::from(name), room);
        }
        self.rooms.get_mut(name).expect("room just inserted")
    }

    pub(crate) fn hot_reload(&mut self, pre: engine_wasm::WasmPreInstance<RoomHostData>) {
        self.pre = Some(pre);
        let pre = self.pre.as_ref().expect("just set by hot_reload");
        for (room_name, room) in &mut self.rooms {
            match room.reload_wasm(pre, self.max_clients) {
                Ok(players) => {
                    tracing::info!(
                        "[server] 🔄 Room '{room_name}' hot-reloaded with {players} active players!"
                    );
                }
                Err(e) => {
                    tracing::error!("[server] ⚠️ Failed to re-instantiate room '{room_name}': {e}");
                }
            }
        }
    }
}
