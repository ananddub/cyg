
use engine_net::transport::GameTransport;

use crate::host::*;

impl<'a> Engine<'a> {
    /// Join options for the guest: schema-encoded when the descriptor carries
    /// a `JoinOptions` def (the generated `decodeJoinOptions` reads this
    /// frame), raw JSON bytes otherwise.
    /// Host-rendered auth context for `onAuth` — what the room can trust.
    /// Starts empty (`{}`); the seat token already rides `options`. Future
    /// fields (client IP, TLS info) land here without another ABI change.
    pub(crate) fn auth_context(&self, _client_id: &str) -> String {
        "{}".to_string()
    }

    pub(crate) fn encode_join_options(&self, options: &str) -> Vec<u8> {
        if self.join_options_fields.is_empty() {
            return options.as_bytes().to_vec();
        }
        engine_schema::encode_join_options(&self.join_options_fields, options)
    }

    /// Merged create options for a brand-new room: the variant's own options
    /// ([rooms.duo] `options`), then the config's `room_options`, then the
    /// creator's joinOrCreate options — later keys win, like Colyseus's
    /// defineRoom defaults ⊕ join options.
    pub(crate) fn build_create_options(&self, join_options: &str, variant: Option<&engine_config::RoomVariantDef>) -> Vec<u8> {
        let mut merged = serde_json::Map::new();
        if let Some(variant) = variant
            && let Some(text) = &variant.options
            && let Ok(map) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(text)
        {
            merged.extend(map);
        }
        if let Some(text) = &self.cfg.room_options
            && let Ok(map) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(text)
        {
            merged.extend(map);
        }
        if let Ok(map) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(join_options)
        {
            merged.extend(map);
        }
        serde_json::to_vec(&merged).unwrap_or_else(|_| join_options.as_bytes().to_vec())
    }

    pub(crate) fn run_connected(&mut self, client_id: &str) -> Option<()> {
        if self.bus.is_draining() {
            tracing::info!("[server] ⛔ {client_id} refused: server is closing");
            reject_room(&mut self.transport, client_id, CLOSING_EVENT);
            return None;
        }
        let raw_room_str = self.transport.room_of(client_id);
        let raw_room = raw_room_str.as_deref().unwrap_or(&self.cfg.name);
        let is_spectator = raw_room.contains("spectator");
        let is_ecs = raw_room.contains("ecs");
        let room_name = sanitize_room_name(raw_room);
        let join_options = self.transport.options_of(client_id).unwrap_or_default();

        if self.server.would_exceed_room_limit(room_name, self.cfg.max_rooms) {
            tracing::error!(
                "[server] ⛔ room limit reached ({}/{}): rejecting {client_id} for '{room_name}'",
                self.server.rooms.len(),
                self.cfg.max_rooms
            );
            reject_room(&mut self.transport, client_id, "room limit reached");
            return None;
        }

        let room = if is_ecs {
            self.server.get_or_create_ecs(room_name, self.arena_mesh.as_ref())
        } else {
            let create = self.build_create_options(&join_options, self.cfg.rooms.get(raw_room));
            match self.server.get_or_create(room_name, &create) {
                Ok(room) => room,
                Err(e) => {
                    tracing::error!(
                        "[server] ⛔ room '{room_name}' unavailable: {e} — rejecting {client_id}"
                    );
                    reject_room(&mut self.transport, client_id, "room unavailable");
                    return None;
                }
            }
        };
        room.empty_since = None;

        self.connected_create(client_id, room_name, is_spectator, join_options)
    }

    pub(crate) fn connected_create(
        &mut self,
        client_id: &str,
        room_name: &str,
        is_spectator: bool,
        join_options: String,
    ) -> Option<()> {
        let options_frame = self.encode_join_options(&join_options);
        // room_key is "<variant>__filters"; the variant name is the prefix.
        let variant_name = room_name.split("__").next().unwrap_or(room_name);
        let variant = self.cfg.rooms.get(variant_name).cloned();

        let auth_context = self.auth_context(client_id);
        let history = EventHistory::new(&self.bus, &self.game_cfg);
        let create_frame = self.build_create_options(&join_options, variant.as_ref());
        let room = self.server.get_or_create(room_name, &create_frame).ok()?;
        if let Some(variant) = variant {
            room.max_clients_override = variant.max_clients;
            room.persistent_override = variant.persistent.unwrap_or(false);
        }
        room.empty_since = None;

        if is_spectator {
            room.spectators.insert(std::sync::Arc::from(client_id));
            tracing::info!(
                "[server] 👁️  {client_id} connected as SPECTATOR to room '{room_name}'"
            );
        } else if let Some(id) = room
            .takeover(client_id)
            .filter(|_| room.sessions.contains_key(client_id))
        {
            tracing::info!(
                "[server] 🔁 {client_id} took over a live session in '{room_name}' (player id {id})"
            );
            flush_room_outbox(room, &mut self.transport, room_name, &history);
        } else if let Some(id) = room.rejoin(client_id) {
            tracing::info!(
                "[server] 🔁 {client_id} reconnected to room '{room_name}' (player id {id})"
            );
            flush_room_outbox(room, &mut self.transport, room_name, &history);
        } else if room.is_full(self.cfg.max_clients) {
            tracing::error!(
                "[server] ⛔ room '{room_name}' is full ({}/{}): rejecting {client_id}",
                room.sessions.len(),
                self.cfg.max_clients
            );
            reject_room(&mut self.transport, client_id, "room is full");
            return None;
        } else if !room.authorize(room.next_player_id, &join_options, &auth_context) {
            tracing::error!("[server] ⛔ room '{room_name}' rejected {client_id} in onAuth");
            reject_room(&mut self.transport, client_id, "not authorized");
            return None;
        } else {
            let (x, y) = {
                let n = room.sessions.len();
                (
                    60.0 + ((n as f64) * 97.0) % (self.cfg.width - 120.0),
                    60.0 + ((n as f64) * 61.0) % (self.cfg.height - 120.0),
                )
            };
            let id = room.join(client_id, x, y, &options_frame);
            tracing::info!("[server] {client_id} joined room '{room_name}' (player id {id})");
            if let Some(user_id) = claimed_user_id(&join_options) {
                history.bridge_seat(room_name, id, Some(user_id));
            }
            flush_room_outbox(room, &mut self.transport, room_name, &history);
        }

        self.transport.flush().ok();
        Some(())
    }
}
