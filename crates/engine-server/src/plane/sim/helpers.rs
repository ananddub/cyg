use std::collections::HashMap;

use engine_bus::PeerId;
use crate::rooms::GameRoom;

pub(super) const SPAWN_W: f64 = 800.0;
pub(super) const SPAWN_H: f64 = 500.0;

#[inline]
pub(super) fn decode_packed(packed: i64) -> (usize, usize) {
    (((packed >> 32) as u32) as usize, (packed as u32) as usize)
}

pub(super) fn bind_peer(
    room: &mut GameRoom,
    peer_id: PeerId,
    room_wanted: &str,
    expected: &str,
    max_players: i32,
    session_of: &mut HashMap<i64, PeerId>,
) -> i64 {
    if room_wanted != expected || room.sessions.len() as i32 >= max_players {
        return -1;
    }
    let n = room.sessions.len();
    let (x, y) = (
        60.0 + ((n as f64) * 97.0) % (SPAWN_W - 120.0),
        60.0 + ((n as f64) * 61.0) % (SPAWN_H - 120.0),
    );
    let id = room.next_player_id;
    room.next_player_id += 1;
    if let Some(ref mut wasm) = room.wasm {
        if let Err(e) = wasm.on_join(id, x, y) {
            tracing::error!("[sim] ⚠️ guest on_join error for player {id}: {e}");
        }
    }
    room.sessions.insert(peer_id.clone(), id);
    session_of.insert(id, peer_id);
    id
}

pub(super) fn drop_peer(
    room: &mut GameRoom,
    peer_id: &PeerId,
    browser_players: &mut std::collections::HashSet<i64>,
    session_of: &mut HashMap<i64, PeerId>,
) {
    if let Some(id) = room.sessions.remove(peer_id) {
        if let Some(ref mut wasm) = room.wasm {
            let _ = wasm.on_leave(id);
        }
        room.last_input_seq.remove(peer_id.as_str());
        browser_players.remove(&id);
        session_of.remove(&id);
        room.input_journal.retain(|j| j.id != id);
    }
}
