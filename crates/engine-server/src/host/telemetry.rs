use std::time::{Duration, Instant};

use engine_bus::{RoomTelemetry, SessionTelemetry};

use crate::host::*;
use crate::telemetry::PublishReport;

impl<'a> Engine<'a> {
    pub(crate) fn run_telemetry(&mut self) {
        if self.stats.publish_start.elapsed() < Duration::from_secs(1) {
            return;
        }
        let publish_start = self.stats.publish_start;
        let window_sec = publish_start.elapsed().as_secs_f64().max(1e-9);
        let mut sessions: Vec<SessionTelemetry> = Vec::new();
        let mut rooms: Vec<RoomTelemetry> = Vec::with_capacity(self.server.rooms.len());
        let mut players = 0u32;
        let mut webrtc_live = 0u32;
        for (room_name, room) in &self.server.rooms {
            players += room.sessions.len() as u32;
            report_ownership(
                &self.bus,
                &self.node_id,
                &self.game_id,
                room_name,
                room,
                room.phase(),
            );
            rooms.push(RoomTelemetry {
                name: room_name.clone(),
                phase: room.phase(),
                clients: room.sessions.len() as u32,
                spectators: room.spectators.len() as u32,
                tick: room.current_tick,
                age_sec: room.born.elapsed().as_secs(),
                closes_in_sec: self.cfg.lifecycle.closes_at(room.born).map(|deadline| {
                    let now = Instant::now();
                    if now >= deadline {
                        0
                    } else {
                        (deadline - now).as_secs()
                    }
                }),
            });
            let members = room.sessions.iter().map(|(s, &id)| (s, Some(id)));
            let watching = room.spectators.iter().map(|s| (s, None));
            for (session, player_id) in members.chain(watching) {
                let live = self.transport.is_rtc_connected(session);
                webrtc_live += live as u32;
                sessions.push(SessionTelemetry {
                    session: session.to_string(),
                    transport: if live { "webrtc" } else { "websocket" }.to_string(),
                    player_id,
                    acked_seq: player_id
                        .and_then(|id| room.seqs.get(&id))
                        .map(|s| s.input as u32),
                    idle_ms: self
                        .rates
                        .last_input_at
                        .get(session.as_ref())
                        .map(|t| t.elapsed().as_millis() as u64),
                });
            }
        }
        rooms.sort_by(|a, b| a.name.cmp(&b.name));
        for (&addr, &(id, ref _sess, last_seen)) in &self.udp.udp_peers {
            sessions.push(SessionTelemetry {
                session: addr.to_string(),
                transport: "native-udp".to_string(),
                player_id: (id > 0).then_some(id),
                acked_seq: None,
                idle_ms: Some(last_seen.elapsed().as_millis() as u64),
            });
        }
        let window_ticks = self.tick.window_ticks;
        let stage_sim_us_sum = self.tick.stage_sim_us_sum;
        let stage_guest_us_sum = self.tick.stage_guest_us_sum;
        let tick_interval = self.clock.tick_interval;
        let session_cap = self.cfg.max_clients.max(0) as usize;
        let ccu = self.transport.ccu() as u32;
        let report = PublishReport {
            stage_sim_us: (stage_sim_us_sum / window_ticks.max(1)) as u64,
            stage_guest_us: (stage_guest_us_sum / window_ticks.max(1)) as u64,
            tick_hz_est: window_ticks as f64 / window_sec,
            tick_rate_hz: live_tick_rate(tick_interval),
            players,
            ws_sessions: ccu.saturating_sub(webrtc_live),
            webrtc_sessions: webrtc_live,
            udp_sessions: self.udp.udp_peers.len() as u32,
            sessions,
            rooms,
            session_cap,
        };
        if self.stats.recorder.maybe_publish(&self.bus, report) {
            self.stats.publish_start = Instant::now();
            self.tick.window_ticks = 0;
            self.tick.stage_sim_us_sum = 0;
            self.tick.stage_guest_us_sum = 0;
        }
    }
}
