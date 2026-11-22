use std::time::Instant;

use crate::host::*;

/// The voice byte and the WebRTC voice opcode are the same number on the same wire.
const _: () = assert!(
    engine_voice::VOICE_MAGIC == engine_webrtc_protocol::OP_VOICE,
    "engine-voice's VOICE_MAGIC and engine-webrtc-protocol's OP_VOICE are one wire byte"
);

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

        // `engine-voice` and the WebRTC frame table both claim byte 0x56 — the voice packet's
        // magic here, `OP_VOICE` there. They are the same wire, so the two must never drift:
        // the assertion below turns a mismatch into a build error instead of a silent
        // "voice packets become engine frames" bug at runtime.
        if data.len() >= engine_voice::HEADER_LEN && data[0] == engine_voice::VOICE_MAGIC {
            // One owned copy serves both decode and the patched wire frame —
            // the old path copied the packet twice per voice packet.
            let mut wire_packet = data.to_vec();
            if let Ok(mut packet) = engine_voice::VoicePacket::decode(&wire_packet[..]) {
                packet.speaker_id = id as u32;
                let routed = room.voice_room.dispatch_packet(&packet, Instant::now());
                wire_packet[1..5].copy_from_slice(&(id as u32).to_be_bytes());
                if !routed.is_empty() {
                    // Reverse index built once per packet (only when frames
                    // routed), not a linear scan of all sessions per frame —
                    // that was O(listeners × sessions) per voice packet.
                    let pid_to_session: std::collections::HashMap<u32, &std::rc::Rc<str>> = room
                        .sessions
                        .iter()
                        .map(|(cid, &pid)| (pid as u32, cid))
                        .collect();
                    for frame in routed {
                        if frame.listener_id == id as u32 {
                            continue; // never echo the speaker back
                        }
                        if let Some(&cid) = pid_to_session.get(&frame.listener_id) {
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

        // Schema input batch: replay the newest unacked frames in one app
        // message. Validate the whole packet first, charge every sample to the
        // rate bucket, then hand the batch to the guest. The seat's held frame
        // is the newest individual frame, never the batch itself, so idle
        // synthesis can bump its lane seq normally.
        let mut guest_data = data;
        if let Some(batch) = netcode::parse_input_batch(data) {
            self.rates.note_input(client_id);
            if !input_budget_allows_n(
                &mut self.rates.rtc_input_rates,
                self.clock.tick_interval,
                client_id,
                batch.frames.len(),
            ) {
                self.stats.recorder.inputs_dropped += 1;
                return;
            }
            if let Some((seq, latest)) = batch.latest_frame() {
                let entry = room.seqs.entry(id).or_insert(crate::rooms::SessionSeqs {
                    input: 0,
                    acked_event: 0,
                    stamp: 0,
                });
                if entry.input == 0 || engine_webrtc_protocol::is_newer(entry.input, seq) {
                    entry.input = seq;
                }
                if let Some(meta) = batch.meta {
                    entry.stamp = meta.stamp;
                }
                room.last_input.insert(id, latest);
                room.inputs_since_tick.insert(id);
            }
        } else {
            self.rates.note_input(client_id);
            // Schema input lane: peel the client's RTT stamp trailer (Phase D
            // echo — the tail of the next delta carries it back) and remember
            // the guest-ready frame as the seat's last input, so hold-last idle
            // synthesis can re-deliver it on an empty tick. Anything else that
            // reaches here is a plain app message, never held.
            let (stripped, _meta) = netcode::extract_input_stamp(&mut room.seqs, id, data);
            // The guest receives the original frame so the bridge can read the
            // client timestamp trailer; `stripped` is only the held idle frame.
            if let Some(seq) = netcode::input_app_seq(data) {
                let entry = room.seqs.entry(id).or_insert(crate::rooms::SessionSeqs {
                    input: 0,
                    acked_event: 0,
                    stamp: 0,
                });
                if entry.input == 0 || engine_webrtc_protocol::is_newer(entry.input, seq) {
                    entry.input = seq;
                }
            }
            if netcode::is_input_app_frame(stripped) {
                room.last_input.insert(id, stripped.to_vec());
                room.inputs_since_tick.insert(id);
            }
        }
        if room.is_ecs() {
            room.ecs_input(&client_id, guest_data);
            flush_room_outbox(room, &mut self.transport, room_name, &history);
            return;
        }

        if let Some(ref mut wasm) = room.wasm {
            match wasm.on_alloc(guest_data.len() as i32) {
                Ok(ptr_i32) => {
                    let ptr = ptr_i32 as u32;
                    if let Err(e) = wasm.write_memory(ptr, guest_data) {
                        tracing::error!("[server] ⚠️ failed to write memory for player {id}: {e}");
                    } else if let Err(e) = wasm.on_input(id, ptr_i32, guest_data.len() as i32) {
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
