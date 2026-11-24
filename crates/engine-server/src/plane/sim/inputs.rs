use std::time::Instant;

use engine_bus::Via;

use crate::netcode;
use crate::plane::mono_ns;
use crate::plane::sim::state::Plane;

impl Plane {
    pub(super) fn drain_inputs(&mut self) {
        let mut drained = 0usize;
        while drained < 64 {
            let Ok(msg) = self.bus.inputs_rx.try_recv() else {
                break;
            };
            drained += 1;
            self.recorder.note_input_latency(msg.arrival_ns, mono_ns());
            let Some(session) = self.session_of.get(&msg.player_id) else {
                self.recorder.inputs_dropped += 1;
                continue;
            };
            if msg.via == Via::Browser {
                self.browser_players.insert(msg.player_id);
            }
            self.last_input_at.insert(session.clone(), Instant::now());
            let dt = self.dt;
            let mut payload = msg.payload;
            if netcode::apply_input(
                &mut self.room,
                &self.plane_game_cfg,
                dt,
                session,
                msg.player_id,
                msg.seq,
                &mut payload,
            )
            .is_err()
            {
                self.recorder.inputs_dropped += 1;
            }
        }
    }
}
