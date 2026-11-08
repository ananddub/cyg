use std::sync::Arc;
use std::time::Instant;

use engine_core::RateDecision;
use engine_net::transport::webrtc::contract::Inbound;

use crate::host::*;

impl<'a> Engine<'a> {
    pub(crate) fn run_inbound(&mut self) {
        self.rates.message_rate_limits.sweep(Instant::now());
        for (client_id, msg) in self.transport.take_inbound() {
            if self.rates.message_rate_limits.is_enabled() {
                match self
                    .rates
                    .message_rate_limits
                    .check(&client_id, Instant::now())
                {
                    RateDecision::Allow => {}
                    RateDecision::Disconnect => {
                        self.stats.recorder.inputs_dropped += 1;
                        tracing::warn!(
                            "[server] 🚫 {client_id} exceeded max_messages_per_second ({}); force-disconnecting",
                            self.cfg.max_messages_per_second
                        );
                        let _ = self.transport.disconnect(&client_id);
                        self.rates.rtc_input_rates.remove(client_id.as_ref());
                        continue;
                    }
                }
            }
            let room_name: Arc<str> = self
                .transport
                .room_of(&client_id)
                .map(|r| {
                    let s = sanitize_room_name(&r);
                    if s.len() == r.len() { r } else { Arc::from(s) }
                })
                .unwrap_or_else(|| Arc::from(self.cfg.name.as_str()));
            let Some(room) = self.server.rooms.get_mut(room_name.as_ref()) else {
                continue;
            };
            let Some(&id) = room.sessions.get(&*client_id) else {
                continue;
            };
            match msg {
                Inbound::Input {
                    seq,
                    payload,
                    resupplied,
                } => {
                    self.rates.note_input(&client_id);
                    if !input_budget_allows(
                        &mut self.rates.rtc_input_rates,
                        self.clock.tick_interval,
                        &client_id,
                    ) {
                        self.stats.recorder.inputs_dropped += 1;
                        continue;
                    }
                    if !resupplied && seq % 120 == 1 {
                        tracing::debug!(
                            "[server] ⚡ WebRTC UDP input stream active for player {id} (seq {seq})"
                        );
                    }
                    if netcode::apply_input(
                        room,
                        self.cfg,
                        self.clock.tick_interval.as_secs_f64(),
                        &client_id,
                        id,
                        seq,
                        &payload,
                    )
                    .is_err()
                    {
                        self.stats.recorder.inputs_dropped += 1;
                    }
                }
                Inbound::Event { raw } => self.inbound_event(&room_name, &client_id, id, &raw),
                Inbound::Ack(ack) => netcode::record_channel_ack(room, &client_id, ack),
                Inbound::App { data } => self.inbound_app(&room_name, &client_id, id, &data),
                Inbound::Other { .. } | Inbound::Refused(_) => {}
            }
        }
    }
}
