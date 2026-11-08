use std::time::Instant;

use crate::host::*;

impl<'a> Engine<'a> {
    pub(crate) fn run_packet(&mut self, client_id: &str, payload: bytes::Bytes) {
        let history = EventHistory::new(&self.bus, &self.game_cfg);
        let Some(data) = strip_type_header(&payload) else {
            return;
        };
        if data.len() > engine_wasm::MAX_INBOUND_PAYLOAD_SIZE {
            tracing::warn!(
                "[server] ⚠️ rejected payload from {client_id}: size {} exceeds max {}",
                data.len(),
                engine_wasm::MAX_INBOUND_PAYLOAD_SIZE
            );
            return;
        }
        let raw_room_str = self.transport.room_of(&client_id);
        let raw_room = raw_room_str.as_deref().unwrap_or(&self.cfg.name);
        let room_name = sanitize_room_name(raw_room);
        let Some(room) = self.server.rooms.get_mut(room_name) else {
            return;
        };
        room.note_activity();

        if data.len() == 5 && data[0] == 0xFF && &data[1..5] == b"SPEC" {
            room.make_spectator(&client_id);
            tracing::info!("[server] 👁️  {client_id} upgraded to SPECTATOR in room '{room_name}'");
            let _ = self
                .transport
                .send_to(&client_id, OpCode::RoomDataBytes, SPECTATOR_CONFIRM);
            return;
        }

        if data.len() >= 2 && data[0] == 0x05 {
            let type_len = data[1] as usize;
            if type_len > 0
                && 2 + type_len <= data.len()
                && &data[2..2 + type_len] == b"__ox_schema_ready"
            {
                room.schema_sent.insert(std::rc::Rc::from(client_id));
                return;
            }
        }

        let Some(&id) = room.sessions.get(&*client_id) else {
            return;
        };

        if engine_voice::SignalFrame::is_signal(data) {
            self.run_packet_signal(&client_id, id, data, &payload, room_name);
            return;
        }

        if data.len() >= engine_voice::HEADER_LEN && data[0] == engine_voice::VOICE_MAGIC {
            let bytes = bytes::Bytes::copy_from_slice(data);
            if let Ok(mut packet) = engine_voice::VoicePacket::decode(&bytes) {
                packet.speaker_id = id as u32;
                let routed = room.voice_room.dispatch_packet(&packet, Instant::now());
                let mut wire_packet = data.to_vec();
                wire_packet[1..5].copy_from_slice(&(id as u32).to_be_bytes());
                for frame in routed {
                    for (cid, &pid) in &room.sessions {
                        if pid == id {
                            continue;
                        }
                        if pid as u32 == frame.listener_id {
                            let _ =
                                self.transport
                                    .send_to(cid, OpCode::RoomDataBytes, &wire_packet);
                        }
                    }
                }
            }
            return;
        }

        // Voice settings: [VOICE_MAGIC][b'T'|b'M'|b'D'][payload...].
        // 'T' carries a client-hashed u32 team id; 'M'/'D' are one byte.
        if data.len() >= 2
            && data[0] == engine_voice::VOICE_MAGIC
            && matches!(data[1], b'T' | b'M' | b'D')
        {
            self.run_packet_voice_settings(id, data, room_name);
            return;
        }

        if let Some(ack) = engine_webrtc_protocol::parse_ack(data) {
            netcode::record_channel_ack(room, &client_id, ack);
            return;
        }

        if let Some(report) = engine_webrtc_protocol::parse_checksum(data) {
            if let Err(desync) = room
                .rollback_buffer
                .verify_sync(report.tick, report.checksum)
            {
                self.stats.recorder.desyncs += 1;
                tracing::warn!(
                    "[server] ⚠️ desync in room '{room_name}': player {id} disagrees at tick {} (host {:016x}, client {:016x})",
                    desync.tick,
                    desync.expected_checksum,
                    desync.client_checksum
                );
                history.record_desync(room_name, id, &desync);
            }
            return;
        }

        if data.len() == engine_webrtc_protocol::EVENT_WIRE_LEN
            && data[0] == engine_webrtc_protocol::OP_EVENT
        {
            self.run_packet_event(id, data, room_name);
            return;
        }

        if data.len() == engine_webrtc_protocol::INPUT_WIRE_LEN
            && data[0] == engine_webrtc_protocol::OP_INPUT
        {
            self.run_packet_input(&client_id, id, data, room_name);
            return;
        }

        if data.len() > 3 && data[0] == netcode::OP_INPUT_BATCH {
            self.run_packet_batch(&client_id, id, data, room_name);
            return;
        }

        self.rates.note_input(client_id);
        if room.is_ecs() {
            room.ecs_input(&client_id, data);
            flush_room_outbox(room, &mut self.transport, room_name, &history);
            return;
        }

        if let Some(ref mut wasm) = room.wasm {
            match wasm.on_alloc(data.len() as i32) {
                Ok(ptr_i32) => {
                    let ptr = ptr_i32 as u32;
                    if let Err(e) = wasm.write_memory(ptr, data) {
                        tracing::error!("[server] ⚠️ failed to write memory for player {id}: {e}");
                    } else if let Err(e) = wasm.on_input(id, ptr_i32, data.len() as i32) {
                        tracing::warn!("[server] ⚠️ guest trap in on_input for player {id}: {e}");
                    }
                }
                Err(e) => {
                    tracing::error!("[server] ⚠️ guest on_alloc failed for player {id}: {e}");
                }
            }
        }
        flush_room_outbox(room, &mut self.transport, room_name, &history);
    }
}
