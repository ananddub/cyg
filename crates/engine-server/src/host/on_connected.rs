
use engine_net::transport::GameTransport;

use crate::host::*;

impl<'a> Engine<'a> {
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
            match self.server.get_or_create(room_name) {
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

        self.connected_create(client_id, room_name, is_spectator)
    }

    pub(crate) fn connected_create(
        &mut self,
        client_id: &str,
        room_name: &str,
        is_spectator: bool,
    ) -> Option<()> {
        let join_options = self.transport.options_of(client_id).unwrap_or_default();

        let history = EventHistory::new(&self.bus, &self.game_cfg);
        let room = self.server.get_or_create(room_name).ok()?;
        room.empty_since = None;

        if is_spectator {
            room.spectators.insert(client_id.to_string());
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
        } else if !room.authorize(room.next_player_id, &join_options) {
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
            let id = room.join(client_id, x, y);
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
