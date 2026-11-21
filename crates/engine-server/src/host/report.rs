use engine_bus::{Bus, MatchFinished, RoomSnapshot};
use engine_net::framing::OpCode;
use engine_net::transport::GameTransport;

use crate::rooms::{GameRoom, GameServer, RoomBackend};

use crate::host::*;
pub(crate) fn report_match(
    bus: &Bus,
    game_id: &str,
    room: &GameRoom,
    name: &str,
    end_reason: &str,
    peak_players: u32,
) {
    let now = unix_secs();
    bus.push_match(MatchFinished {
        room: name.to_string(),
        game_id: game_id.to_string(),
        end_reason: end_reason.to_string(),
        started_at_unix: now - room.lifecycle.born.elapsed().as_secs() as i64,
        ended_at_unix: now,
        ticks: room.current_tick as i64,
        peak_players: peak_players as i64,
        snapshot_seq: room.snapshot_seq as i64,
        result: room.result.as_deref().unwrap_or_default().to_vec(),
    });
}

pub(crate) fn report_ownership(
    bus: &Bus,
    node: &str,
    game_id: &str,
    name: &str,
    room: &GameRoom,
    state: engine_bus::RoomPhase,
) {
    bus.push_ownership(engine_bus::RoomOwnership {
        room: name.to_string(),
        node: node.to_string(),
        game: game_id.to_string(),
        state,
        peers: (room.sessions.len() + room.spectators.len()) as i64,
        tick: room.current_tick as i64,
        snapshot_seq: room.snapshot_seq as i64,
    });
}

pub(crate) fn backend_label(backend: RoomBackend) -> &'static str {
    match backend {
        RoomBackend::Relay => "relay",
        RoomBackend::Wasm => "wasm",
        RoomBackend::Ecs => "ecs",
    }
}

pub(crate) fn report_snapshot(bus: &Bus, room: &GameRoom, name: &str, bytes: &[u8]) {
    bus.push_snapshot(RoomSnapshot {
        room: name.to_string(),
        backend: backend_label(room.backend).to_string(),
        seq: room.snapshot_seq as i64,
        bytes: bytes.to_vec(),
    });
}

pub(crate) fn announce_closure(server: &GameServer, transport: &mut LoopTransport) {
    let mut frame = Vec::with_capacity(24);
    frame.push(APP_MESSAGE_OPCODE);
    frame.push(CLOSING_EVENT.len() as u8);
    frame.extend_from_slice(CLOSING_EVENT.as_bytes());
    frame.push(0x80);
    for room in server.rooms.values() {
        let mut targets: Vec<&str> =
            Vec::with_capacity(room.sessions.len() + room.spectators.len());
        targets.extend(room.sessions.keys().map(|s| s.as_ref()));
        targets.extend(room.spectators.iter().map(|s| s.as_ref()));
        let _ = transport.send_to_many(&targets, OpCode::RoomDataBytes, &frame);
    }
    let _ = transport.flush();
}
