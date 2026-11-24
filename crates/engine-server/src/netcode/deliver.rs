use crate::rooms::{GameRoom, JournalInput, RoomHostData, RoomLogic, SessionSeqs};

use super::input::{InputRejection, MAX_SEQ_AHEAD, sanitize_input_payload};

/// Immediate host→guest delivery: alloc, write, `on_input`. The rollback
/// replay path calls this directly — replayed frames must reach the guest
/// even when live inputs are being staged for tick-time batching. The
/// payload is whatever the seat's lane produces: the 20-byte mesh frame or
/// the schema lane's app frame.
pub fn deliver_input_now(wasm: Option<&mut RoomLogic<RoomHostData>>, id: i64, payload: &[u8]) {
    if let Some(wasm) = wasm {
        let len = payload.len() as i32;
        if let Ok(ptr_i32) = wasm.on_alloc(len) {
            let _ = wasm.write_memory(ptr_i32 as u32, payload);
            let _ = wasm.on_input(id, ptr_i32, len);
        }
    }
}

pub fn deliver_input(room: &mut GameRoom, id: i64, payload: &[u8; 20]) {
    if room.input_coalesce {
        room.stage_input(id, payload.to_vec());
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
    payload: &mut [u8; 20],
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
    // Sanitize IN PLACE, never reject (Colyseus `sanitize` rule): the frame
    // the guest, the journal and the rollback replay all see is the SAME
    // sanitized copy, so honest clients (whose values were already in
    // range) can predict without diverging from the host's view.
    sanitize_input_payload(payload, &room.input_domains);

    let last = room.seqs.get(&id).map(|s| s.input).unwrap_or(0);

    if last == 0 || engine_webrtc_protocol::is_newer(last, seq) {
        let jump = seq.wrapping_sub(last);
        if last != 0 && jump > MAX_SEQ_AHEAD {
            tracing::warn!(
                "[server] ⏩ re-anchoring player {id}: input seq jumped {jump} samples ({last} -> {seq})"
            );
        }
        room.seqs
            .entry(id)
            .or_insert(SessionSeqs {
                input: seq,
                acked_event: 0,
                stamp: 0,
            })
            .input = seq;
        room.last_input.insert(id, payload.to_vec());
        room.inputs_since_tick.insert(id);
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
