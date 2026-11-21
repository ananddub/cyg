use crate::host::*;

impl<'a> Engine<'a> {
    pub(crate) fn run_packet_input(
        &mut self,
        client_id: &str,
        id: i64,
        data: &[u8],
        room_name: &str,
    ) {
        if let Ok(input) = engine_webrtc_protocol::InputFrame::decode(data) {
            self.rates.note_input(client_id);
            if !input_budget_allows(
                &mut self.rates.rtc_input_rates,
                self.clock.tick_interval,
                client_id,
            ) {
                self.stats.recorder.inputs_dropped += 1;
                return;
            }
            if self.transport.is_rtc_connected(client_id) && input.seq % 120 == 1 {
                tracing::debug!(
                    "[server] ℹ️ Player {id} sent a stamped input frame (seq {})",
                    input.seq
                );
            }
            let mut payload_20 = [0u8; 20];
            payload_20.copy_from_slice(&data[3..23]);
            let room = self.server.rooms.get_mut(room_name).unwrap();
            let cfg = self.cfg;
            let tick_secs = self.clock.tick_interval.as_secs_f64();
            if netcode::apply_input(
                room,
                cfg,
                tick_secs,
                client_id,
                id,
                input.seq,
                &mut payload_20,
            )
                .is_err()
            {
                self.stats.recorder.inputs_dropped += 1;
            }
        }
    }

    pub(crate) fn run_packet_batch(
        &mut self,
        client_id: &str,
        id: i64,
        data: &[u8],
        room_name: &str,
    ) {
        if let Some(samples) = netcode::batch_sample_count(data) {
            self.rates.note_input(client_id);
            let room = self.server.rooms.get_mut(room_name).unwrap();
            let cfg = self.cfg;
            for i in 0..samples {
                if !input_budget_allows(
                    &mut self.rates.rtc_input_rates,
                    self.clock.tick_interval,
                    client_id,
                ) {
                    self.stats.recorder.inputs_dropped += 1;
                    break;
                }
                let off = 3 + i * 22;
                let seq = u16::from_le_bytes([data[off], data[off + 1]]);
                let mut payload_20 = [0u8; 20];
                payload_20.copy_from_slice(&data[off + 2..off + 22]);
                if netcode::apply_input(
                    room,
                    cfg,
                    self.clock.tick_interval.as_secs_f64(),
                    client_id,
                    id,
                    seq,
                    &mut payload_20,
                )
                .is_err()
                {
                    self.stats.recorder.inputs_dropped += 1;
                }
            }
        }
    }
}
