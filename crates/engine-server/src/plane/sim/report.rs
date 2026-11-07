use std::collections::{HashMap, HashSet};
use std::time::Instant;

use engine_bus::{PeerId, RoomTelemetry, SessionTelemetry};

use crate::plane::sim::state::Plane;
use crate::rooms::GameRoom;
use crate::telemetry::PublishReport;

pub(super) fn publish_report(
    room_name: &str,
    room: &GameRoom,
    browser_players: &HashSet<i64>,
    last_input_at: &HashMap<PeerId, Instant>,
    tick_hz: f64,
    tick_hz_est: f64,
    stage_sim_us: u64,
    stage_guest_us: u64,
    max_players: i32,
) -> PublishReport {
    let mut sessions = Vec::with_capacity(room.sessions.len());
    let mut browser = 0u32;
    for (peer_id, &player_id) in &room.sessions {
        let via_browser = browser_players.contains(&player_id);
        browser += via_browser as u32;
        sessions.push(SessionTelemetry {
            session: peer_id.clone(),
            transport: if via_browser { "webrtc" } else { "native-udp" }.to_string(),
            player_id: Some(player_id),
            acked_seq: room.seqs.get(&player_id).map(|s| s.input as u32),
            idle_ms: last_input_at
                .get(peer_id)
                .map(|t| t.elapsed().as_millis() as u64),
        });
    }
    let players = room.sessions.len() as u32;
    PublishReport {
        stage_sim_us,
        stage_guest_us,
        tick_hz_est,
        tick_rate_hz: tick_hz.max(0.0) as u32,
        players,
        ws_sessions: 0,
        webrtc_sessions: browser,
        udp_sessions: players.saturating_sub(browser),
        sessions,
        rooms: vec![RoomTelemetry {
            name: room_name.to_string(),
            phase: room.phase(),
            clients: players,
            spectators: room.spectators.len() as u32,
            tick: room.current_tick,
            age_sec: room.born.elapsed().as_secs(),
            closes_in_sec: None,
        }],
        session_cap: max_players.max(0) as usize,
    }
}

impl Plane {
    pub(super) fn report(&self, stage_sim: u64, stage_guest: u64) -> PublishReport {
        publish_report(
            &self.cfg.room_name,
            &self.room,
            &self.browser_players,
            &self.last_input_at,
            self.cfg.tick_hz,
            self.measured_hz,
            stage_sim,
            stage_guest,
            self.cfg.max_players,
        )
    }
}
