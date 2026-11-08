use std::time::Duration;

use crate::host::*;
use crate::rooms::close_code;

impl<'a> Engine<'a> {
    pub(crate) fn run_disconnected(&mut self, client_id: &str) {
        let history = EventHistory::new(&self.bus, &self.game_cfg);

        tracing::info!("[server] disconnected: {client_id}");
        self.rates.last_input_at.remove(&*client_id);
        self.rates.rtc_input_rates.remove(&*client_id);
        self.rates.message_rate_limits.remove(&client_id);
        let raw_room_str = self.transport.take_room_of(client_id);
        let room_name = if let Some(ref r) = raw_room_str {
            sanitize_room_name(r).to_string()
        } else {
            let mut found = None;
            for (rname, r) in &self.server.rooms {
                if r.sessions.contains_key(&*client_id) || r.spectators.contains(&*client_id) {
                    found = Some(rname.to_string());
                    break;
                }
            }
            found.unwrap_or_else(|| self.cfg.name.clone())
        };
        self.transport.retain_session(&room_name, client_id);
        let Some(room) = self.server.rooms.get_mut(room_name.as_str()) else {
            return;
        };
        let consented = self.transport.take_consented(client_id);
        let seat_before = room.sessions.get(&*client_id).copied();
        let gone = if consented {
            room.handle_disconnect(client_id)
        } else {
            // Unexpected: the room gets onDrop(id, LOST) first — its window
            // to message others / mark the player "dropped" — then the
            // reconnection hold starts.
            let dropped = room
                .sessions
                .get(&*client_id)
                .copied()
                .unwrap_or(room.next_player_id);
            room.guest_on_drop(dropped, close_code::LOST);
            room.handle_unexpected_disconnect(
                client_id,
                Duration::from_secs(self.cfg.lifecycle.seat_hold_secs),
            )
        };
        if let Some(seat) = seat_before {
            if !room.sessions.contains_key(&*client_id)
                && !room.pending_leaves.contains_key(&*client_id)
            {
                history.bridge_seat(&room_name, seat, None);
            }
        }
        if let Some(gone) = gone {
            announce_voice_peer_change(
                &mut self.transport,
                &mut room.voice_mesh,
                engine_voice::SignalKind::PeerLeft,
                gone,
            );
        }
        flush_room_outbox(room, &mut self.transport, &room_name, &history);
        self.transport.rtc_server().remove_session(client_id);
        if room.is_hibernating() {
            tracing::info!(
                "[server] ⏸️ Room '{room_name}' is now empty -> hibernating (0% CPU, {}s auto-close timer started)",
                self.cfg.lifecycle.reap_idle_secs
            );
        }
    }
}
