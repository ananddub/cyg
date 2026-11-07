use crate::rooms::{GameRoom, JournalInput, RoomHostData, RoomLogic, SessionSeqs};

use super::input::{InputRejection, MAX_SEQ_AHEAD, validate_input_payload};

/// Immediate host→guest delivery: alloc, write, `on_input`. The rollback
/// replay path calls this directly — replayed frames must reach the guest
/// even when live inputs are being staged for tick-time batching.
pub fn deliver_input_now(
    wasm: Option<&mut RoomLogic<RoomHostData>>,
    id: i64,
    payload: &[u8; 20],
) {
    if let Some(wasm) = wasm {
        if let Ok(ptr_i32) = wasm.on_alloc(20) {
            let _ = wasm.write_memory(ptr_i32 as u32, payload);
            let _ = wasm.on_input(id, ptr_i32, 20);
        }
    }
}

pub fn deliver_input(room: &mut GameRoom, id: i64, payload: &[u8; 20]) {
    if room.input_coalesce {
        room.stage_input(id, *payload);
        return;
    }
    deliver_input_now(room.wasm.as_mut(), id, payload);
}

pub fn apply_input(
    room: &mut GameRoom,
    cfg: &engine_config::GameConfig,
    dt: f64,
    session: &str,
    id: i64,
    seq: u16,
    payload: &[u8; 20],
) -> Result<(), InputRejection> {
    let session_key = match room.sessions.get_key_value(session) {
        Some((key, &seated)) if seated == id => key.clone(),
        _ => {
            tracing::warn!(
                "[server] ⛔ {session} drove player {id}, which it does not seat — input refused"
            );
            return Err(InputRejection::NotSeated);
        }
    };
    if !validate_input_payload(payload) {
        tracing::warn!(
            "[server] ⛔ malformed input from {session} (player {id}, seq {seq}) — input refused"
        );
        return Err(InputRejection::Malformed);
    }

    let last = room.seqs.get(&id).map(|s| s.input).unwrap_or(0);

    if last == 0 || engine_webrtc_protocol::is_newer(last, seq) {
        let jump = seq.wrapping_sub(last);
        if last != 0 && jump > MAX_SEQ_AHEAD {
            tracing::warn!(
                "[server] ⏩ re-anchoring player {id}: input seq jumped {jump} samples ({last} -> {seq})"
            );
        }
        room.seqs.entry(id).or_insert(SessionSeqs { input: seq, acked_event: 0 }).input = seq;
        deliver_input(room, id, payload);
        let tick = room.current_tick;
        room.input_journal.push_back(JournalInput {
            tick,
            session: session_key,
            id,
            seq,
            payload: *payload,
        });
        while room.input_journal.len() > cfg.rollback.journal_size {
            room.input_journal.pop_front();
        }
        return Ok(());
    }

    super::rollback::apply_late(room, cfg, dt, &session_key, id, seq, payload, last)
}
