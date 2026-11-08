use crate::host::*;

impl<'a> Engine<'a> {
    pub(crate) fn run_packet_signal_server(
        &mut self,
        client_id: &str,
        from: u32,
        signal: &engine_voice::SignalFrame,
    ) {
        let webrtc = self.transport.rtc_server();
        match signal.kind {
            engine_voice::SignalKind::Offer => {
                let sdp_str =
                    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&signal.payload) {
                        v.get("sdp").and_then(|s| s.as_str()).map(|s| s.to_string())
                    } else {
                        std::str::from_utf8(&signal.payload)
                            .ok()
                            .map(|s| s.to_string())
                    };
                if let Some(sdp) = sdp_str {
                    match webrtc.handle_offer(client_id, &sdp) {
                        Ok(answer_sdp) => {
                            let answer_body = serde_json::json!({
                                "sdp": answer_sdp,
                                "type": "answer"
                            })
                            .to_string();
                            let ans_frame = engine_voice::SignalFrame::new(
                                engine_voice::SignalKind::Answer,
                                engine_voice::SERVER_SENDER,
                                from,
                                bytes::Bytes::from(answer_body),
                            )
                            .encode();
                            let _ = self.transport.send_to(
                                client_id,
                                OpCode::RoomDataBytes,
                                &ans_frame,
                            );
                            tracing::info!(
                                "[server] ⚡ WebRTC Client-Server offer accepted for {client_id}, SDP answer returned"
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                "[server] ⚠️ WebRTC handle_offer error for client {client_id}: {e}"
                            );
                        }
                    }
                }
            }
            engine_voice::SignalKind::Ice => {
                if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&signal.payload) {
                    if let Some(cand) = v.get("candidate").and_then(|c| c.as_str()) {
                        match webrtc.handle_candidate(client_id, cand) {
                            Ok(_) => tracing::info!(
                                "[server] 🧊 Client {client_id} added ICE candidate: {cand}"
                            ),
                            Err(e) => tracing::error!(
                                "[server] ⚠️ Client {client_id} candidate error: {e} ({cand})"
                            ),
                        }
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn run_packet_event(&mut self, id: i64, data: &[u8], room_name: &str) {
        let room = self.server.rooms.get_mut(room_name).unwrap();
        if let Ok(mut event) = engine_webrtc_protocol::EventFrame::decode(data) {
            room.event_seq = room.event_seq.wrapping_add(1);
            event.seq = room.event_seq;
            event.from = id as u16;

            let mut wire_event = Vec::with_capacity(engine_webrtc_protocol::EVENT_WIRE_LEN);
            engine_webrtc_protocol::encode_event(&event, &mut wire_event);

            for (cid, &pid) in &room.sessions {
                if pid != id {
                    let _ = self
                        .transport
                        .send_to(cid, OpCode::RoomDataBytes, &wire_event);
                }
            }
            if !room.spectators.is_empty() {
                for spec in &room.spectators {
                    let _ = self
                        .transport
                        .send_to(spec, OpCode::RoomDataBytes, &wire_event);
                }
            }
        }
    }
}
