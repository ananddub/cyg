use crate::rooms::*;
use std::time::Instant;

impl GameRoom {

    pub(crate) fn release_player(&mut self, session: &str, id: i64) {
        if let Some(ref mut ecs) = self.ecs {
            ecs.remove_player(session);
        }
        if let Some(ref mut wasm) = self.wasm {
            let _ = wasm.on_leave(id);
        }
        self.voice_room.leave_participant(id as u32);
        self.last_input_seq.remove(session);
        self.input_journal.retain(|j| j.session != session);
    }

    #[allow(dead_code)]
    pub(crate) fn spawn_point(&self, width: f64, height: f64) -> (f64, f64) {
        let n = self.sessions.len();
        (
            60.0 + ((n as f64) * 97.0) % (width - 120.0),
            60.0 + ((n as f64) * 61.0) % (height - 120.0),
        )
    }

    pub(crate) fn join(&mut self, session: &str, x: f64, y: f64) -> i64 {
        let id = self.next_player_id;
        self.next_player_id += 1;
        self.empty_since = None;
        if let Some(ref mut ecs) = self.ecs {
            ecs.add_player(session, id, x, y);
        }
        if let Some(ref mut wasm) = self.wasm {
            if let Err(e) = wasm.on_join(id, x, y) {
                tracing::error!("[server] ⚠️ guest on_join error for player {id}: {e}");
            }
        }
        self.voice_room
            .join_participant(id as u32, Some(1), [x, 0.0, y]);
        self.sessions.insert(session.to_string(), id);
        id
    }

    #[allow(dead_code)]
    pub(crate) fn join_spectator(&mut self, session: &str) {
        self.empty_since = None;
        self.spectators.insert(session.to_string());
    }

    pub(crate) fn make_spectator(&mut self, session: &str) {
        if let Some(id) = self.sessions.remove(session) {
            if let Some(ref mut wasm) = self.wasm {
                let _ = wasm.on_leave(id);
            }
        }
        self.schema_sent.remove(session);
        self.spectators.insert(session.to_string());
    }

    pub(crate) fn add_raw_player(&mut self, x: f64, y: f64) -> i64 {
        let id = self.next_player_id;
        self.next_player_id += 1;
        self.empty_since = None;
        if let Some(ref mut wasm) = self.wasm {
            let _ = wasm.on_join(id, x, y);
        }
        id
    }

    pub(crate) fn remove_raw_player(&mut self, id: i64) {
        if let Some(ref mut wasm) = self.wasm {
            let _ = wasm.on_leave(id);
        }
    }

    pub(crate) fn detach_session(&mut self, session: &str) -> Option<u32> {
        self.spectators.remove(session);
        self.schema_sent.remove(session);
        self.acked_event_seq.remove(session);
        self.voice_mesh.leave_session(session)
    }

    pub(crate) fn arm_empty_timer(&mut self) {
        if self.sessions.is_empty() && self.spectators.is_empty() {
            self.empty_since = Some(Instant::now());
        }
    }

}
