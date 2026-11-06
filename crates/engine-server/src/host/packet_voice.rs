
use crate::host::*;

impl<'a> Engine<'a> {
    pub(crate) fn run_packet_signal(
        &mut self,
        client_id: &str,
        id: i64,
        data: &[u8],
        payload: &bytes::Bytes,
        room_name: &str,
    ) {
        if !self
            .server
            .rooms
            .get(room_name)
            .is_some_and(|room| room.voice_enabled)
        {
            return; // [features] voice = false — drop at the door
        }
        let wire = payload.slice(payload.len() - data.len()..);
        let Ok(signal) = engine_voice::SignalFrame::decode(&wire) else {
            tracing::warn!("[server] ⚠️ dropped malformed signal from {client_id}");
            return;
        };
        let from = id as u32;

        match signal.kind {
            engine_voice::SignalKind::Join => {
                let mut body = String::from(r#"{"iceServers":"#);
                body.push_str(&self.cfg.voice_ice_servers);
                body.push_str(r#","peers":["#);
                let peers = self.server.rooms[room_name].voice_mesh.peers_sorted();
                for (i, &peer) in peers.iter().enumerate() {
                    if i > 0 {
                        body.push(',');
                    }
                    body.push_str(&peer.to_string());
                }
                body.push_str(r#"],"self":"#);
                body.push_str(&from.to_string());
                body.push('}');

                let hello = engine_voice::SignalFrame::new(
                    engine_voice::SignalKind::Config,
                    engine_voice::SERVER_SENDER,
                    from,
                    bytes::Bytes::from(body),
                )
                .encode();
                let _ = self
                    .transport
                    .send_to(client_id, OpCode::RoomDataBytes, &hello);

                let room = self.server.rooms.get_mut(room_name).unwrap();
                if room.voice_mesh.join(from, client_id) {
                    announce_voice_peer_change(
                        &mut self.transport,
                        &mut room.voice_mesh,
                        engine_voice::SignalKind::PeerJoined,
                        from,
                    );
                }
            }
            engine_voice::SignalKind::Bye => {
                let room = self.server.rooms.get_mut(room_name).unwrap();
                if room.voice_mesh.leave(from).is_some() {
                    announce_voice_peer_change(
                        &mut self.transport,
                        &mut room.voice_mesh,
                        engine_voice::SignalKind::PeerLeft,
                        from,
                    );
                }
            }
            _kind if signal.to == engine_voice::SERVER_SENDER => {
                self.run_packet_signal_server(client_id, from, &signal);
                return;
            }
            kind if kind.is_relayed() => {
                let room = self.server.rooms.get_mut(room_name).unwrap();
                if signal.to == engine_voice::BROADCAST
                    || signal.to == from
                    || !room.voice_mesh.contains(signal.to)
                {
                    reject_signal(
                        &mut self.transport,
                        client_id,
                        from,
                        "unknown voice peer target",
                    );
                    return;
                }
                let out = engine_voice::rewrite_sender(&wire, from);
                room.voice_mesh.route(from, signal.to, |target| {
                    let _ = self
                        .transport
                        .send_to(target.session_id, OpCode::RoomDataBytes, &out);
                });
            }
            other => {
                tracing::warn!("[server] ⚠️ {client_id} sent a server-only signal ({other:?})");
            }
        }
    }
    pub(crate) fn run_packet_voice_settings(&mut self, id: i64, data: &[u8], room_name: &str) {
        if data.len() < 2 {
            return;
        }
        let room = self.server.rooms.get_mut(room_name).unwrap();
        match data[1] {
            b'T' => {
                // Team is a client-hashed FNV-1a u32 (same label → same bucket
                // on every client). 0 means "leave the team". Older 3-byte
                // numeric squad packets are still accepted as raw ids.
                let team_id = if data.len() >= 6 {
                    let raw = u32::from_be_bytes([data[2], data[3], data[4], data[5]]);
                    (raw != 0).then_some(raw)
                } else if data.len() >= 3 {
                    Some(data[2] as u32)
                } else {
                    None
                };
                room.voice_room.set_team(id as u32, team_id);
                tracing::info!("[server] 🛡️ Player {id} switched to Squad {team_id:?}");
            }
            b'M' if data.len() >= 3 => {
                let muted = data[2] == 1;
                room.voice_room.set_muted(id as u32, muted);
                tracing::info!("[server] 🎙️ Player {id} mic muted={muted}");
                if room.voice_mesh.peer_count() > 1 {
                    let note = engine_voice::SignalFrame::new(
                        engine_voice::SignalKind::Muted,
                        engine_voice::SERVER_SENDER,
                        engine_voice::BROADCAST,
                        bytes::Bytes::from(format!("[{id},{}]", muted as u8)),
                    )
                    .encode();
                    room.voice_mesh.for_each_peer(id as u32, |target| {
                        let _ = self
                            .transport
                            .send_to(target.session_id, OpCode::RoomDataBytes, &note);
                    });
                }
            }
            b'D' if data.len() >= 3 => {
                let deafened = data[2] == 1;
                room.voice_room.set_deafened(id as u32, deafened);
                tracing::info!("[server] 🎧 Player {id} deafened={deafened}");
            }
            _ => {}
        }
    }
}
