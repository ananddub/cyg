use crate::rooms::{GameRoom, JournalInput};

use super::deliver::deliver_input_now;
use super::input::InputRejection;

pub(super) fn apply_late(
    room: &mut GameRoom,
    cfg: &engine_config::GameConfig,
    dt: f64,
    session: &std::rc::Rc<str>,
    id: i64,
    seq: u16,
    payload: &[u8; 20],
    last: u16,
) -> Result<(), InputRejection> {
    if !cfg.rollback.enabled || seq == last || room.wasm.is_none() {
        return Ok(());
    }
    if room
        .input_journal
        .iter()
        .any(|j| j.session == *session && j.seq == seq)
    {
        return Ok(());
    }
    let Some(pred_idx) = room
        .input_journal
        .iter()
        .rposition(|j| j.session == *session && j.seq == seq.wrapping_sub(1))
    else {
        tracing::debug!(
            "[dbg] late seq {seq}: no predecessor in journal ({} entries)",
            room.input_journal.len()
        );
        return Ok(());
    };
    let pred_tick = room.input_journal[pred_idx].tick;
    let gap = room.current_tick.saturating_sub(pred_tick);
    if gap == 0 || gap > cfg.rollback.max_replay_ticks {
        return Ok(());
    }

    let Some(ckpt) = room.rollback_buffer.blob_for(pred_tick) else {
        return Ok(());
    };
    let Some(ref mut wasm) = room.wasm else {
        return Ok(());
    };
    let Ok(load_ptr_i32) = wasm.on_alloc(ckpt.len() as i32) else {
        tracing::error!("[server] ⚠️ rollback replay dropped: guest on_alloc failed");
        return Ok(());
    };
    let load_ptr = load_ptr_i32 as u32 as usize;
    if let Err(e) = wasm.write_memory(load_ptr as u32, ckpt) {
        tracing::error!("[server] ⚠️ rollback replay dropped: checkpoint write failed: {e}");
        return Ok(());
    }
    if let Err(e) = wasm.on_load(load_ptr as i32, ckpt.len() as i32) {
        tracing::warn!("[server] ⚠️ rollback replay dropped: guest trap in on_load: {e}");
        return Ok(());
    }

    let mut items: Vec<JournalInput> = room.input_journal.drain(..).collect();
    items.insert(
        pred_idx + 1,
        JournalInput {
            tick: pred_tick,
            session: session.clone(),
            id,
            seq,
            payload: *payload,
        },
    );
    room.input_journal.extend(items);

    let target = room.current_tick;
    let mut cur = pred_tick;
    let mut redelivered = 0usize;
    while cur < target {
        let batch: Vec<(i64, [u8; 20])> = room
            .input_journal
            .iter()
            .filter(|j| j.tick == cur)
            .map(|j| (j.id, j.payload))
            .collect();
        redelivered += batch.len();
        for (jid, jpayload) in batch {
            deliver_input_now(room.wasm.as_mut(), jid, &jpayload);
        }
        if let Some(ref mut wasm) = room.wasm {
            if let Ok(packed) = wasm.on_tick(dt) {
                cur += 1;
                let sptr = ((packed >> 32) as u32) as usize;
                let slen = (packed as u32) as usize;
                if let Ok(blob) = wasm.read_memory(sptr as u32, slen as u32) {
                    let _ = room.rollback_buffer.save_blob(cur, blob);
                }
            } else {
                break;
            }
        } else {
            break;
        }
    }

    tracing::debug!(
        "[server] ⏪ late input seq {seq} (player {id}): rewound {} ticks, re-delivered {redelivered} inputs, resimulated to tick {cur}",
        target - pred_tick
    );
    Ok(())
}
