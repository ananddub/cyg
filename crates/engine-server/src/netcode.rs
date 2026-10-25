//! Delta envelope, AOI delta filtering, and input journaling/rollback.
//!
//! The pure helpers of the room pipeline. The host run loop calls these; this
//! module owns no state and no sockets — it only shapes bytes and rewinds rooms.

use std::time::Instant;

use engine_net::transport::WebRtcServer;

use crate::rooms::{GameRoom, JournalInput};

/// Schema state payload magic. Kept in sync with
/// `sdk/interfaces/assemblyscript/openoxide.ts` and `sdk/web/openoxide.js`.
pub const SCHEMA_MAGIC: &[u8; 4] = b"OXS2";
/// Per-tick schema envelope: magic + fullLen + full + patchLen + patch.
pub const SCHEMA_ENVELOPE_HEADER: usize = 8;
/// Full/patch section: magic + kind + schema hash + count.
pub const SCHEMA_SECTION_HEADER: usize = 9;

/// The two halves of a guest schema `onTick` payload.
pub struct SchemaSections<'a> {
    /// Complete row for a viewer that has never seen this state.
    pub full: &'a [u8],
    /// Field-level changes for a viewer that already applied `full`.
    pub patch: &'a [u8],
}

/// Split the guest's schema envelope into its full and patch sections.
///
/// Returns `None` for legacy `DeltaWriter` payloads and for malformed
/// envelopes, so a bad guest buffer is broadcast as-is rather than panicking
/// the host loop.
pub fn split_schema(buf: &[u8]) -> Option<SchemaSections<'_>> {
    if buf.len() < SCHEMA_ENVELOPE_HEADER || &buf[0..4] != SCHEMA_MAGIC {
        return None;
    }
    let full_len = u32::from_le_bytes(buf[4..8].try_into().ok()?) as usize;
    let full_start = SCHEMA_ENVELOPE_HEADER;
    let full_end = full_start.checked_add(full_len)?;
    if full_end + 4 > buf.len() {
        return None;
    }
    let patch_len = u32::from_le_bytes(buf[full_end..full_end + 4].try_into().ok()?) as usize;
    let patch_start = full_end + 4;
    let patch_end = patch_start.checked_add(patch_len)?;
    if patch_end != buf.len() {
        return None;
    }
    let full = &buf[full_start..full_end];
    let patch = &buf[patch_start..patch_end];
    if full.len() < SCHEMA_SECTION_HEADER
        || patch.len() < SCHEMA_SECTION_HEADER
        || &full[0..4] != SCHEMA_MAGIC
        || &patch[0..4] != SCHEMA_MAGIC
    {
        return None;
    }
    Some(SchemaSections { full, patch })
}

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
    if let Some(ref mut wasm) = room.wasm {
        if let Ok(ptr_i32) = wasm.on_alloc(20) {
            let _ = wasm.write_memory(ptr_i32 as u32, payload);
            let _ = wasm.on_input(id, ptr_i32, 20);
        }
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

    if !cfg.rollback.enabled || seq == *last || room.wasm.is_none() {
        return; // legacy newest-wins drop or pure unity relay mode
    }
    // Duplicate resend — it is already in the journal.
    if room
        .input_journal
        .iter()
        .any(|j| j.session == session && j.seq == seq)
    {
        return;
    }
    // The input slots in right after its predecessor. No predecessor inside the
    // journal window means we cannot place it; drop it like the old behavior did.
    let Some(pred_idx) = room
        .input_journal
        .iter()
        .rposition(|j| j.session == session && j.seq == seq.wrapping_sub(1))
    else {
        tracing::debug!(
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
    let Some(ref mut wasm) = room.wasm else {
        return;
    };
    // Rule 5 (fault isolation): this path is client-reachable (any out-of-order
    // seq can trigger a rewind), so a guest trap here must drop the replay,
    // never kill the host loop.
    let Ok(load_ptr_i32) = wasm.on_alloc(ckpt.len() as i32) else {
        tracing::error!("[server] ⚠️ rollback replay dropped: guest on_alloc failed");
        return;
    };
    let load_ptr = load_ptr_i32 as u32 as usize;
    if let Err(e) = wasm.write_memory(load_ptr as u32, ckpt) {
        tracing::error!("[server] ⚠️ rollback replay dropped: checkpoint write failed: {e}");
        return;
    }
    if let Err(e) = wasm.on_load(load_ptr as i32, ckpt.len() as i32) {
        tracing::warn!("[server] ⚠️ rollback replay dropped: guest trap in on_load: {e}");
        return;
    }

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
        if let Some(ref mut wasm) = room.wasm {
            if let Ok(packed) = wasm.on_tick(dt) {
                cur += 1;
                // Guest serializes its own state; host stores the blob.
                let sptr = ((packed >> 32) as u32) as usize;
                let slen = (packed as u32) as usize;
                if let Ok(blob) = wasm.read_memory(sptr as u32, slen as u32) {
                    let _ = room.rollback_buffer.save_blob(cur, blob);
                }
            } else {
                break; // guest trapped: stop replay, state is pred_tick-rewound
            }
        } else {
            break;
        }
    }

    tracing::debug!(
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
pub fn parse_delta_sections(delta: &[u8]) -> Option<(u32, Vec<DeltaEntity>, u32, usize, usize)> {
    let (ver, entities, bullets, (bc_off, bullet_end)) = engine_spatial::aoi::parse_delta(delta)?;
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
/// this delta on the WebRTC delta lane" — no open channel, or a body too large
/// to carry — and the caller falls back to the reliable lane, which is ordered
/// and needs no stamping.
///
/// A partially sent delta is reported as zero. Retrying it is pointless: the
/// next tick supersedes it, so an incomplete body is worthless, and the frames
/// that did go out are cleaned up by the receiver's fragment expiry.
pub fn send_delta_framed(
    webrtc_server: &mut WebRtcServer,
    session: &str,
    seq: u16,
    tick: u32,
    body: &[u8],
) -> usize {
    match webrtc_server.send_delta(session, seq, tick, body) {
        Ok(report) => {
            if report.delivered() {
                report.fragments
            } else {
                0
            }
        }
        Err(e) => {
            tracing::error!("[server] ⚠️ delta refused for {session}: {e} — dropped");
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
    let slot = room
        .acked_event_seq
        .entry(client_id.to_string())
        .or_insert(ack.seq);
    if engine_webrtc_protocol::is_newer(*slot, ack.seq) {
        *slot = ack.seq;
    }
}

/// Canonical Unity CYG OpCodes
pub const CYG_OP_LEAVE_ROOM: u8 = 3;


/// Wire-protocol *header* codes (`CYGWireProtocol` in
/// `sdk/unity/Runtime/Transport/CYGTransport.cs`). These are the bytes the
/// transport writes in the frame header — they are **not** the canonical
/// opcodes above. `10` is JOIN_ROOM, `11` is ERROR, `12` is LEAVE_ROOM; and
/// PING is `18`, not `0`.
pub const CYG_WIRE_JOIN_ROOM: u8 = 10;
pub const CYG_WIRE_LEAVE_ROOM: u8 = 12;
pub const CYG_WIRE_PING: u8 = 18;

/// Reliable-message wire op (native UDP RUDP): arbitrary payload, guaranteed
/// delivery (Go-Back-N resends until ACKed). Frame `[30][seq u16 LE][payload]`
/// — same shape both directions (client chat/shoot/spell AND server events).
pub const CYG_WIRE_RELIABLE_MSG: u8 = 30;
/// Reliable ack: `[31][seq u16 LE]` — cumulative ("everything ≤ seq arrived").
pub const CYG_WIRE_RELIABLE_ACK: u8 = 31;

/// Encode one reliable frame: `[30][seq][payload]`.
pub fn encode_reliable_msg(seq: u16, payload: &[u8], out: &mut Vec<u8>) {
    out.clear();
    out.push(CYG_WIRE_RELIABLE_MSG);
    out.extend_from_slice(&seq.to_le_bytes());
    out.extend_from_slice(payload);
}

/// Decode a reliable frame. `None` for any other shape.
pub fn decode_reliable_msg(data: &[u8]) -> Option<(u16, &[u8])> {
    if data.len() < 4 || data[0] != CYG_WIRE_RELIABLE_MSG {
        return None;
    }
    let seq = u16::from_le_bytes([data[1], data[2]]);
    Some((seq, &data[3..]))
}

/// Encode one reliable ack: `[31][seq]`.
pub fn encode_reliable_ack(seq: u16, out: &mut Vec<u8>) {
    out.clear();
    out.push(CYG_WIRE_RELIABLE_ACK);
    out.extend_from_slice(&seq.to_le_bytes());
}

/// Extended (SACK) ack: `[31][cum u16][count u8]([start u16][end u16])*` —
/// "everything ≤ cum arrived, PLUS these disjoint ranges above cum". The
/// sender resends only what neither covers; already-delivered frames above a
/// gap are never resent. A 3-byte ack (no count byte) remains valid — old
/// clients keep working, no ranges.
pub fn encode_reliable_ack_sack(
    cum: u16,
    ranges: &[(u16, u16)],
    out: &mut Vec<u8>,
) {
    out.clear();
    out.push(CYG_WIRE_RELIABLE_ACK);
    out.extend_from_slice(&cum.to_le_bytes());
    out.push(ranges.len().min(255) as u8);
    for &(s, e) in ranges.iter().take(255) {
        out.extend_from_slice(&s.to_le_bytes());
        out.extend_from_slice(&e.to_le_bytes());
    }
}

/// Decode a reliable ack (both shapes). Returns `(cumulative, ranges)`.
/// `None` for any other opcode/shape.
pub fn decode_reliable_ack(data: &[u8]) -> Option<(u16, Vec<(u16, u16)>)> {
    if data.len() < 3 || data[0] != CYG_WIRE_RELIABLE_ACK {
        return None;
    }
    let cum = u16::from_le_bytes([data[1], data[2]]);
    if data.len() == 3 {
        return Some((cum, Vec::new()));
    }
    let Some(&count) = data.get(3) else {
        return Some((cum, Vec::new()));
    };
    let mut ranges = Vec::with_capacity(count as usize);
    for i in 0..count as usize {
        let base = 4 + i * 4;
        let Some(start) = data.get(base..base + 2) else { break };
        let Some(end) = data.get(base + 2..base + 4) else { break };
        ranges.push((
            u16::from_le_bytes([start[0], start[1]]),
            u16::from_le_bytes([end[0], end[1]]),
        ));
    }
    Some((cum, ranges))
}

/// Small token bucket used to cap packets per peer/session. It is
/// deliberately tiny and allocation-free after construction.
#[derive(Clone, Debug)]
pub struct InputRateLimiter {
    tokens: f64,
    rate_per_sec: f64,
    burst: f64,
    last: Instant,
}

impl InputRateLimiter {
    pub fn new(rate_per_sec: u32, burst: f64) -> Self {
        let rate = rate_per_sec.max(1) as f64;
        let burst = burst.max(1.0);
        Self {
            tokens: burst,
            rate_per_sec: rate,
            burst,
            last: Instant::now(),
        }
    }

    /// Refill by elapsed wall time and consume one token.
    pub fn allow(&mut self, now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.last).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.rate_per_sec).min(self.burst);
        self.last = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

impl Default for InputRateLimiter {
    fn default() -> Self {
        Self::new(256, 32.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limiter_rejects_a_flood() {
        let mut limiter = InputRateLimiter::new(10, 2.0);
        let now = Instant::now();
        assert!(limiter.allow(now));
        assert!(limiter.allow(now));
        assert!(!limiter.allow(now), "burst exhausted");
        assert!(limiter.allow(now + std::time::Duration::from_millis(250)));
    }

    fn schema_section(kind: u8, hash: u32, body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(SCHEMA_MAGIC);
        out.push(kind);
        out.extend_from_slice(&hash.to_le_bytes());
        out.extend_from_slice(body);
        out
    }

    fn schema_envelope(full: &[u8], patch: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(SCHEMA_MAGIC);
        out.extend_from_slice(&(full.len() as u32).to_le_bytes());
        out.extend_from_slice(full);
        out.extend_from_slice(&(patch.len() as u32).to_le_bytes());
        out.extend_from_slice(patch);
        out
    }

    #[test]
    fn schema_envelope_splits_full_and_patch() {
        let full = schema_section(1, 0xC737_1660, &[1, 0, 0, 0]);
        let patch = schema_section(2, 0xC737_1660, &[0, 0, 0, 0]);
        let env = schema_envelope(&full, &patch);
        let sections = split_schema(&env).expect("valid envelope");
        assert_eq!(sections.full, full.as_slice());
        assert_eq!(sections.patch, patch.as_slice());
    }

    #[test]
    fn legacy_delta_and_truncated_schema_are_not_split() {
        assert!(split_schema(&[0, 0, 0, 0]).is_none());
        let full = schema_section(1, 7, &[0, 0, 0, 0]);
        let patch = schema_section(2, 7, &[0, 0, 0, 0]);
        let mut env = schema_envelope(&full, &patch);
        env.pop();
        assert!(
            split_schema(&env).is_none(),
            "truncated envelope must fall back"
        );
    }

    #[test]
    fn schema_envelope_never_panics_on_malformed_input() {
        let full = schema_section(3, 0xC737_1660, &[1, 0, 0, 0]);
        let patch = schema_section(4, 0xC737_1660, &[2, 0, 0, 0]);
        let env = schema_envelope(&full, &patch);

        for end in 0..env.len() {
            assert!(
                split_schema(&env[..end]).is_none(),
                "prefix {end} must not split"
            );
        }

        let mut overflow = env.clone();
        overflow[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(split_schema(&overflow).is_none());

        let mut trailing = env.clone();
        trailing.push(0);
        assert!(split_schema(&trailing).is_none());

        // Deterministic mutation sweep: the splitter must reject or accept,
        // never index out of bounds.
        let mut state = 0x9E37_79B9u32;
        for _ in 0..2_000 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let mut mutated = env.clone();
            let position = (state as usize) % mutated.len();
            mutated[position] ^= (state >> 24) as u8 | 1;
            let _ = split_schema(&mutated);
        }

        let mut random = Vec::new();
        for length in 0..=512 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            random.clear();
            for _ in 0..length {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                random.push((state >> 16) as u8);
            }
            let _ = split_schema(&random);
        }
    }
}

#[cfg(test)]
mod rudp_tests {
    use super::*;
    use crate::plane::reliable::{ReliableLane, MAX_RELIABLE_BYTES};

    /// Wire roundtrip: encode → decode byte-identical, arbitrary sizes.
    #[test]
    fn reliable_msg_wire_roundtrip_arbitrary_sizes() {
        let mut out = Vec::new();
        for size in [1usize, 9, 64, 512, MAX_RELIABLE_BYTES] {
            let payload: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
            encode_reliable_msg(0xBEEF, &payload, &mut out);
            assert_eq!(out[0], CYG_WIRE_RELIABLE_MSG);
            let (seq, got) = decode_reliable_msg(&out).expect("decodes");
            assert_eq!(seq, 0xBEEF);
            assert_eq!(got, payload.as_slice());
        }
        // ack roundtrip
        encode_reliable_ack(1247, &mut out);
        assert_eq!(decode_reliable_ack(&out), Some((1247, vec![])));
        // non-matches
        assert!(decode_reliable_msg(&[0x05, 1, 0, 9]).is_none(), "wrong op");
        assert!(decode_reliable_ack(&[0x05, 1, 0]).is_none(), "wrong op");
        assert!(decode_reliable_ack(&[0x1F, 1]).is_none(), "short ack");
        assert!(decode_reliable_msg(&[0x1E, 1]).is_none(), "empty payload");
    }

    /// The delivery guarantee: a datagram that "never arrives" (send returns
    /// false = drop) is resent on the next pump, and a cumulative ack retires
    /// the whole window.
    #[test]
    fn lane_resends_on_loss_and_acks_cumulatively() {
        let mut lane = ReliableLane::new();
        let mut seq = 1u16;
        for i in 0..3 {
            let payload = format!("chat-{i}");
            let mut frame = Vec::new();
            encode_reliable_msg(seq, payload.as_bytes(), &mut frame);
            lane.queue(seq, frame, 0).expect("queues");
            seq += 1;
        }
        // pump 1: sab sent_once
        let mut sent = 0;
        lane.pump(0, |_| {
            sent += 1;
            true
        });
        assert_eq!(sent, 3);

        // pump 2 @ t=5000 (RTO ke baad): pehla send fail (drop) — baaki bhi
        // attempt hote hain Go-Back-N me; simulate only first delivered
        let mut attempts = 0;
        lane.pump(5000, |_| {
            attempts += 1;
            attempts == 1 // first goes out, "rest fail"
        });
        assert!(attempts >= 1, "resend happened after loss");

        // cumulative ack(3): poori window release
        let released = lane.ack(3, 6000);
        assert_eq!(released, 3);
        assert_eq!(lane.inflight(), 0);
    }
}

#[cfg(test)]
mod quic_semantics_tests {
    use super::*;
    use std::collections::HashSet;

    /// QUIC-style receive semantics the host now uses: frames deliver
    /// immediately whatever their order; only resend-duplicates are dropped.
    /// (Symmetric simulation of the host's dedupe ring.)
    #[test]
    fn reordered_frames_all_deliver_and_duplicates_suppress() {
        let mut seen: HashSet<u16> = HashSet::new();
        let mut delivered: Vec<u16> = Vec::new();
        let recv = |seq: u16, seen: &mut HashSet<u16>, delivered: &mut Vec<u16>| {
            if seen.insert(seq) {
                delivered.push(seq);
            }
        };

        // Network reordering: 3 arrives before 1 and 2 — QUIC me koi block nahi.
        recv(3, &mut seen, &mut delivered);
        recv(1, &mut seen, &mut delivered);
        recv(2, &mut seen, &mut delivered);
        assert_eq!(delivered, vec![3, 1, 2], "sab deliver, order-of-arrival me");

        // Sender ka resend (1 dobara aaya) — duplicate, suppress.
        recv(1, &mut seen, &mut delivered);
        assert_eq!(delivered.len(), 3, "duplicate nahi deliver hua");

        // Ek missing frame bhi doosron ko block nahi karta: 5 aaya, 4 abhi
        // network me hai — 5 turant deliver.
        recv(5, &mut seen, &mut delivered);
        assert_eq!(delivered.len(), 4);
    }
}

#[cfg(test)]
mod sack_tests {
    use super::*;
    use crate::plane::reliable::ReliableLane;

    fn frame(seq: u16) -> Vec<u8> {
        let mut f = Vec::new();
        encode_reliable_msg(seq, &[seq as u8; 16], &mut f);
        f
    }

    /// THE SACK payoff: 1 lost, 2 & 3 delivered -> sender window me sirf 1
    /// bachta hai (2,3 release) — resend sirf 1 ka, mila-hua dobara nahi.
    #[test]
    fn sack_releases_delivered_frames_above_the_gap() {
        let mut lane = ReliableLane::new();
        for seq in 1..=3u16 {
            lane.queue(seq, frame(seq), 0).expect("queues");
        }
        // t=0 pe sab ek baar gaye; frame 1 network me gayab.
        lane.pump(0, |_| true);

        // Client SACK: cum=0 (1 missing hai), ranges=[(2,3)].
        let released = lane.ack_selective(0, &[(2, 3)], 100);
        assert_eq!(released, 2, "2 aur 3 release hue");
        assert_eq!(lane.inflight(), 1, "sirf genuinely-missing 1 window me");

        // Resend pump: SIRF frame 1 jaata hai.
        let mut sent_seqs = Vec::new();
        lane.pump(5000, |wire| {
            if let Some((seq, _)) = decode_reliable_msg(wire) {
                sent_seqs.push(seq);
            }
            true
        });
        assert_eq!(sent_seqs, vec![1], "sirf missing frame resend, 2/3 nahi");
    }

    /// Wire roundtrip teeno shapes: 3-byte legacy, SACK multi-range, decode
    /// robustness (truncated range tail).
    #[test]
    fn sack_wire_roundtrip_and_backcompat() {
        let mut out = Vec::new();
        // legacy
        encode_reliable_ack(9, &mut out);
        assert_eq!(decode_reliable_ack(&out), Some((9, vec![])));
        // sack: cum=1, ranges [(3,4),(8,8)]
        encode_reliable_ack_sack(1, &[(3, 4), (8, 8)], &mut out);
        assert_eq!(decode_reliable_ack(&out), Some((1, vec![(3, 4), (8, 8)])));
        // count zyada, bytes kam (truncated tail) — jo mile uthe, crash nahi
        let mut bad = vec![CYG_WIRE_RELIABLE_ACK, 1, 0, 2, 3, 0, 4, 0];
        assert_eq!(decode_reliable_ack(&bad), Some((1, vec![(3, 4)])));
        bad.truncate(5);
        assert_eq!(decode_reliable_ack(&bad), Some((1, vec![])));
    }

    /// rxd_summary host-side compute: {1,2,4,5,9} -> cum=2, ranges [(4,5),(9,9)].
    #[test]
    fn rxd_summary_builds_frontier_and_islands() {
        let mut seen = std::collections::HashSet::new();
        for s in [1u16, 2, 4, 5, 9] {
            seen.insert(s);
        }
        let (cum, ranges) = crate::host::rxd_summary_for_test(&seen);
        assert_eq!(cum, 2);
        assert_eq!(ranges, vec![(4, 5), (9, 9)]);
    }
}
