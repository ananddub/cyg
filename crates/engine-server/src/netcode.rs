//! Delta envelope, AOI delta filtering, and input journaling/rollback.
//!
//! The pure helpers of the room pipeline. The host run loop calls these; this
//! module owns no state and no sockets — it only shapes bytes and rewinds rooms.

use engine_net::transport::WebRtcServer;

use crate::rooms::{GameRoom, JournalInput};

/// Wrap a raw delta buffer into a ROOM_DATA_BYTES frame payload:
/// [msgpack fixstr "delta"][i64 recipient_player_id][raw bytes...].
/// Frames into a caller-owned buffer so a 128 Hz broadcast allocates nothing.
pub fn wrap_delta_into(
    out: &mut Vec<u8>,
    your_id: i64,
    delta: &[u8],
    acked_seq: u16,
    server_tick: u32,
) {
    out.clear();
    out.reserve(delta.len() + 22);
    out.push(0xA5);
    out.extend_from_slice(b"delta");
    out.extend_from_slice(&your_id.to_le_bytes());
    out.extend_from_slice(delta);
    // Tail: last input seq the server applied from this session + authoritative tick.
    // Count-driven parsers (Unity/Godot/mesh) ignore it; the web SDK reads it for
    // prediction reconciliation.
    out.extend_from_slice(&acked_seq.to_le_bytes());
    out.extend_from_slice(&server_tick.to_le_bytes());
}

/// Hand one 20-byte movement input to the guest and journal it.
pub fn deliver_input(room: &mut GameRoom, id: i64, payload: &[u8; 20]) {
    if let Ok(ptr_i32) = room.wasm.on_alloc(20) {
        let _ = room.wasm.write_memory(ptr_i32 as u32, payload);
        let _ = room.wasm.on_input(id, ptr_i32, 20);
    }
}

/// Apply a movement input from one session.
///
/// Newest seqs flow straight through, exactly like the legacy newest-wins rule.
/// A seq *behind* the newest is either a redundant resend (already journaled —
/// dropped) or a genuinely late delivery: instead of dropping it,
/// rewind the guest to the snapshot the input's predecessor arrived after,
/// re-deliver every journaled input from that point, and resimulate the missed
/// ticks. The guest must be deterministic for the replay to land cleanly;
/// a trap mid-replay stops at the last good tick and the log says so.
pub fn apply_input(
    room: &mut GameRoom,
    cfg: &engine_config::GameConfig,
    dt: f64,
    session: &str,
    id: i64,
    seq: u16,
    payload: &[u8; 20],
) {
    let last = room.last_input_seq.entry(session.to_string()).or_insert(0);

    if *last == 0 || engine_webrtc_protocol::is_newer(*last, seq) {
        *last = seq;
        deliver_input(room, id, payload);
        let tick = room.current_tick;
        room.input_journal.push_back(JournalInput {
            tick,
            session: session.to_string(),
            id,
            seq,
            payload: *payload,
        });
        while room.input_journal.len() > cfg.rollback.journal_size {
            room.input_journal.pop_front();
        }
        return;
    }

    if !cfg.rollback.enabled || seq == *last {
        return; // legacy newest-wins drop
    }
    // Duplicate resend — it is already in the journal.
    if room.input_journal.iter().any(|j| j.session == session && j.seq == seq) {
        return;
    }
    // The input slots in right after its predecessor. No predecessor inside the
    // journal window means we cannot place it; drop it like the old behavior did.
    let Some(pred_idx) = room
        .input_journal
        .iter()
        .rposition(|j| j.session == session && j.seq == seq.wrapping_sub(1))
    else {
        eprintln!(
            "[dbg] late seq {seq}: no predecessor in journal ({} entries)",
            room.input_journal.len()
        );
        return;
    };
    let pred_tick = room.input_journal[pred_idx].tick;
    let gap = room.current_tick.saturating_sub(pred_tick);
    if gap == 0 || gap > cfg.rollback.max_replay_ticks {
        return;
    }

    // Restore the guest from the checkpoint blob via the guest's own
    // save()/load() contract — no raw linear-memory copy in the host.
    let Some(ckpt) = room.rollback_buffer.blob_for(pred_tick) else {
        return; // checkpoint aged out of the ring window
    };
    let load_ptr = room
        .wasm
        .on_alloc(ckpt.len() as i32)
        .expect("wasm alloc ckpt") as u32 as usize;
    room.wasm.write_memory(load_ptr as u32, ckpt).expect("write ckpt");
    room.wasm
        .on_load(load_ptr as i32, ckpt.len() as i32)
        .expect("wasm on_load");

    // Insert the late input right behind its predecessor, keeping the journal
    // ordered by arrival tick.
    let mut items: Vec<JournalInput> = room.input_journal.drain(..).collect();
    items.insert(
        pred_idx + 1,
        JournalInput {
            tick: pred_tick,
            session: session.to_string(),
            id,
            seq,
            payload: *payload,
        },
    );
    room.input_journal.extend(items);

    // Resimulate pred_tick..current_tick, re-delivering each tick's inputs
    // before the tick that consumed them.
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
            deliver_input(room, jid, &jpayload);
        }
        if let Ok(packed) = room.wasm.on_tick(dt) {
            cur += 1;
            // Guest serializes its own state; host stores the blob.
            let sptr = ((packed >> 32) as u32) as usize;
            let slen = (packed as u32) as usize;
            if let Ok(blob) = room.wasm.read_memory(sptr as u32, slen as u32) {
                let _ = room.rollback_buffer.save_blob(cur, blob);
            }
        } else {
            break; // guest trapped: stop replay, state is pred_tick-rewound
        }
    }

    eprintln!(
        "[server] ⏪ late input seq {seq} (player {id}): rewound {} ticks, re-delivered {redelivered} inputs, resimulated to tick {cur}",
        target - pred_tick
    );
}

/// One 72-byte player record from the tank-3d delta layout:
/// `[id i64][x y z yaw pitch hp score f64×7][state u32][pad u32]`.
///
/// The canonical shape lives in `engine_spatial::aoi`; re-exported here so
/// the host plane call sites keep their `netcode::` prefix.
pub use engine_spatial::aoi::DeltaEntity;

/// Parse the delta body into typed sections. The wire layout is versioned by
/// the guest (FORMAT_VERSION=1): `[u8 ver][u32 count][72B × count]` then
/// `[u32 bullet_count][32B × bullet_count]`. A malformed or foreign layout
/// yields `None`, and the caller falls back to the unfiltered multicast.
///
/// Thin wrapper over [`engine_spatial::aoi::parse_delta`] — that is the one
/// implementation; this flattens the bullets range into the tuple the host
/// plane consumes.
pub fn parse_delta_sections(
    delta: &[u8],
) -> Option<(u32, Vec<DeltaEntity>, u32, usize, usize)> {
    let (ver, entities, bullets, (bc_off, bullet_end)) =
        engine_spatial::aoi::parse_delta(delta)?;
    Some((ver, entities, bullets, bc_off, bullet_end))
}

/// Interest management for one viewer: entities inside
/// `near_radius` are always visible; those inside `far_radius` ride every
/// `skip_ticks`-th tick; everything beyond `far_radius` is culled. The viewer
/// and its own projectiles are never culled. The KD-tree is keyed by entity
/// slot index, not player id, so negative (bot) ids cannot collide.
///
/// Thin wrapper over [`engine_spatial::aoi::filter_for_viewer`].
pub fn filter_delta_for_viewer(
    entities: &[DeltaEntity],
    bullet_count: u32,
    bullets_range: (usize, usize),
    delta: &[u8],
    viewer_id: i64,
    viewer_pos: Option<[f64; 3]>,
    tick: u64,
    aoi: &engine_config::AoiConfig,
    out: &mut Vec<u8>,
) {
    engine_spatial::aoi::filter_for_viewer(
        entities,
        bullet_count,
        bullets_range,
        delta,
        viewer_id,
        viewer_pos,
        tick,
        aoi,
        out,
    );
}

/// Stamp and fragment a delta body for the `game-unreliable` DataChannel: one
/// UDP datagram per frame, each carrying the room-wide `seq` and the `tick` it
/// belongs to.
///
/// Returns the number of frames written. Zero means "this client did not get
/// this delta over WebRTC" — no open channel, or a body too large to carry — and
/// the caller falls back to the WebSocket, which is already ordered and
/// reliable and needs no stamping.
///
/// A partially sent delta is reported as zero. Retrying it is pointless: the
/// next tick supersedes it, so an incomplete body is worthless, and the frames
/// that did go out are cleaned up by the receiver's fragment expiry.
pub fn send_delta_framed(
    webrtc_server: &mut Option<WebRtcServer>,
    session: &str,
    seq: u16,
    tick: u32,
    body: &[u8],
) -> usize {
    let Some(srv) = webrtc_server.as_mut() else {
        return 0;
    };
    match srv.send_delta(session, seq, tick, body) {
        Ok(report) => {
            if report.delivered() {
                report.fragments
            } else {
                0
            }
        }
        Err(e) => {
            eprintln!("[server] ⚠️ delta refused for {session}: {e} — dropped");
            0
        }
    }
}

/// Record that a client holds a reliable frame.
///
/// Forward-only: SCTP can re-deliver and a client that reconnects may replay an
/// old ack, so the high-water mark is what a resync would resume from, not the
/// last number that arrived.
pub fn record_channel_ack(room: &mut GameRoom, client_id: &str, ack: engine_webrtc_protocol::Ack) {
    let slot = room.acked_event_seq.entry(client_id.to_string()).or_insert(ack.seq);
    if engine_webrtc_protocol::is_newer(*slot, ack.seq) {
        *slot = ack.seq;
    }
}
