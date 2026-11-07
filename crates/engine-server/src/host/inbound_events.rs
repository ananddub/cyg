use engine_net::framing::OpCode;

use crate::host::*;

impl<'a> Engine<'a> {
    pub(crate) fn inbound_event(&mut self, room_name: &str, _client_id: &str, id: i64, raw: &[u8]) {
        let Ok(mut event) = engine_webrtc_protocol::EventFrame::decode(raw) else {
            return;
        };
        let room = match self.server.rooms.get_mut(room_name) {
            Some(room) => room,
            None => return,
        };
        room.event_seq = room.event_seq.wrapping_add(1);
        event.seq = room.event_seq;
        event.from = id as u16;

        let mut wire_event = Vec::with_capacity(engine_webrtc_protocol::EVENT_WIRE_LEN);
        engine_webrtc_protocol::encode_event(&event, &mut wire_event);

        if event.kind == engine_webrtc_protocol::EventKind::Fire {
            let fire_payload = 16u32.to_le_bytes();
            if let Some(ref mut wasm) = room.wasm {
                if let Ok(ptr) = wasm.on_alloc(4) {
                    let _ = wasm.write_memory(ptr as u32, &fire_payload);
                    let _ = wasm.on_input(id, ptr, 4);
                }
            }
        }

        self.transport.rtc_server().broadcast_reliable(&wire_event);

        for (cid, &pid) in &room.sessions {
            if pid != id && !self.transport.is_rtc_connected(cid) {
                let _ = self.transport.send_to(cid, OpCode::RoomDataBytes, &wire_event);
            }
        }
    }

    pub(crate) fn inbound_app(&mut self, room_name: &str, client_id: &str, id: i64, data: &[u8]) {
        let history = EventHistory::new(&self.bus, &self.game_cfg);
        let room = match self.server.rooms.get_mut(room_name) {
            Some(room) => room,
            None => return,
        };
        let type_len = data.get(1).copied().unwrap_or(0) as usize;
        if type_len > 0
            && 2 + type_len <= data.len()
            && &data[2..2 + type_len] == b"__ox_schema_ready"
        {
            room.schema_sent.insert(std::sync::Arc::from(client_id));
            return;
        }
        if let Some(ref mut wasm) = room.wasm {
            match wasm.on_alloc(data.len() as i32) {
                Ok(ptr_i32) => {
                    let ptr = ptr_i32 as u32;
                    if let Err(e) = wasm.write_memory(ptr, data) {
                        tracing::error!(
                            "[server] ⚠️ app payload write failed for player {id}: {e}"
                        );
                    } else if let Err(e) = wasm.on_input(id, ptr_i32, data.len() as i32) {
                        tracing::warn!(
                            "[server] ⚠️ guest trap in on_input (app msg) for player {id}: {e}"
                        );
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
