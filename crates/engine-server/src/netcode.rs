//! Delta envelope, AOI delta filtering, and input journaling/rollback.
//!
//! The pure helpers of the room pipeline. The host run loop calls these; this
//! module owns no state and no sockets — it only shapes bytes and rewinds rooms.

use std::time::Instant;

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
    let Some(ref mut wasm) = room.wasm else {
        return;
    };
    let load_ptr = wasm
        .on_alloc(ckpt.len() as i32)
        .expect("wasm alloc ckpt") as u32 as usize;
    wasm.write_memory(load_ptr as u32, ckpt).expect("write ckpt");
    wasm
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

/// Canonical Unity CYG OpCodes
pub const CYG_OP_PING: u8 = 0;
pub const CYG_OP_PONG: u8 = 1;
pub const CYG_OP_JOIN_ROOM: u8 = 2;
pub const CYG_OP_LEAVE_ROOM: u8 = 3;
pub const CYG_OP_PLAYER_INPUT: u8 = 4;
pub const CYG_OP_STATE_DELTA: u8 = 5;
pub const CYG_OP_CUSTOM_RPC: u8 = 6;
pub const CYG_OP_REWIND_HIT: u8 = 7;
pub const CYG_OP_STATE_SNAPSHOT: u8 = 8;
pub const CYG_OP_PROTO_JOIN_ROOM: u8 = 10;
pub const CYG_OP_PROTO_LEAVE_ROOM: u8 = 12;
pub const CYG_OP_PROTO_ROOM_DATA: u8 = 13;
pub const CYG_OP_PROTO_ROOM_STATE: u8 = 14;
pub const CYG_OP_PROTO_STATE_PATCH: u8 = 15;
pub const CYG_OP_PROTO_DATA_BYTES: u8 = 17;
pub const CYG_OP_SPAWN_OBJECT: u8 = 20;
pub const CYG_OP_DESPAWN_OBJECT: u8 = 21;

/// RPC routing targets
pub const RPC_TARGET_SERVER: u8 = 0;
pub const RPC_TARGET_OTHERS: u8 = 1;
pub const RPC_TARGET_ALL: u8 = 2;
pub const RPC_TARGET_OWNER: u8 = 3;

/// Wire-protocol *header* codes (`CYGWireProtocol` in
/// `sdk/unity/Runtime/Transport/CYGTransport.cs`). These are the bytes the
/// transport writes in the frame header — they are **not** the canonical
/// opcodes above. `10` is JOIN_ROOM, `11` is ERROR, `12` is LEAVE_ROOM; and
/// PING is `18`, not `0`.
pub const CYG_WIRE_JOIN_ROOM: u8 = 10;
pub const CYG_WIRE_ERROR: u8 = 11;
pub const CYG_WIRE_LEAVE_ROOM: u8 = 12;
pub const CYG_WIRE_ROOM_DATA: u8 = 13;
pub const CYG_WIRE_ROOM_STATE: u8 = 14;
pub const CYG_WIRE_ROOM_STATE_PATCH: u8 = 15;
pub const CYG_WIRE_ROOM_DATA_BYTES: u8 = 17;
/// reference-inspired Tick Input-Hub opcode. Deliberately distinct from PING
/// (18), which the transport owns and echoes. Never reuse 18 here: doing so
/// turns every heartbeat into an input packet and every input packet into a
/// heartbeat.
pub const CYG_WIRE_INPUT_TICK: u8 = 19;
pub const CYG_WIRE_PING: u8 = 18;
pub const CYG_WIRE_ROOM_REQUEST: u8 = 21;
pub const CYG_WIRE_ROOM_RESPONSE: u8 = 22;

/// Minimum header length for `CYG_WIRE_INPUT_TICK`:
/// 1 byte opcode + 8 bytes room_id + 4 bytes client_tick + 2 bytes player_actor_id + 1 byte input_len = 16 bytes.
pub const WIRE_INPUT_TICK_HEADER_LEN: usize = 16;

/// One Unity `CYGPlayerInput` is 9 bytes. Redundancy carries at most the
/// current input plus the previous two, so only 9/18/27-byte payloads are
/// valid. Zero-length and arbitrary payloads are rejected instead of being
/// recorded and fanned out as if they were real inputs.
pub const INPUT_HUB_INPUT_SIZE: usize = 9;
pub const INPUT_HUB_MAX_REDUNDANCY: usize = 3;
pub const INPUT_HUB_MAX_PACKET_HZ: u32 = 256;
pub const INPUT_HUB_RATE_BURST: f64 = 32.0;

/// Parsed `CYG_WIRE_INPUT_TICK` (opcode 19) packet frame:
/// `[opcode: 19 (1 byte)] [room_id: 8 bytes] [client_tick: 4 bytes (u32)] [player_actor_id: 2 bytes (u16)] [input_len: 1 byte] [input_payload: N bytes]`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WireInputTick<'a> {
    pub room_id: [u8; 8],
    pub client_tick: u32,
    pub player_actor_id: u16,
    pub input_len: u8,
    pub input_payload: &'a [u8],
}

impl<'a> WireInputTick<'a> {
    pub const HEADER_LEN: usize = WIRE_INPUT_TICK_HEADER_LEN;

    /// Parse a `CYG_WIRE_INPUT_TICK` packet from raw bytes.
    /// Returns `None` if data is shorter than 16 bytes, opcode is not 19,
    /// or data length is not exactly 16 + input_len.
    #[inline]
    pub fn parse(data: &'a [u8]) -> Option<Self> {
        if data.len() < Self::HEADER_LEN {
            return None;
        }
        if data[0] != CYG_WIRE_INPUT_TICK {
            return None;
        }
        let mut room_id = [0u8; 8];
        room_id.copy_from_slice(&data[1..9]);
        let client_tick = u32::from_be_bytes([data[9], data[10], data[11], data[12]]);
        let player_actor_id = u16::from_be_bytes([data[13], data[14]]);
        let input_len = data[15];
        let payload = &data[16..];
        // Exact framing: trailing bytes are not part of the packet and must
        // not be amplified to every peer in the room.
        if payload.len() != input_len as usize {
            return None;
        }
        Some(Self {
            room_id,
            client_tick,
            player_actor_id,
            input_len,
            input_payload: payload,
        })
    }

    /// True when the payload is exactly a Unity input frame: the current
    /// 9-byte input plus zero, one, or two 9-byte redundant predecessors.
    #[inline]
    pub fn has_valid_input_payload(&self) -> bool {
        let len = self.input_len as usize;
        self.input_payload.len() == len
            && (INPUT_HUB_INPUT_SIZE..=INPUT_HUB_INPUT_SIZE * INPUT_HUB_MAX_REDUNDANCY)
                .contains(&len)
            && len.is_multiple_of(INPUT_HUB_INPUT_SIZE)
    }

    /// Room id as a u64 big-endian integer.
    #[inline]
    pub fn room_id_u64(&self) -> u64 {
        u64::from_be_bytes(self.room_id)
    }

    /// Room id as a string slice if valid UTF-8.
    #[inline]
    pub fn room_id_str(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.room_id)
    }

    /// Encode into buffer:
    /// `[opcode: 19 (1 byte)] [room_id: 8 bytes] [client_tick: 4 bytes (u32)] [player_actor_id: 2 bytes (u16)] [input_len: 1 byte] [input_payload: N bytes]`
    pub fn encode(
        room_id: [u8; 8],
        client_tick: u32,
        player_actor_id: u16,
        input_payload: &[u8],
        out: &mut Vec<u8>,
    ) {
        let len = input_payload.len().min(255) as u8;
        out.reserve(Self::HEADER_LEN + len as usize);
        out.push(CYG_WIRE_INPUT_TICK);
        out.extend_from_slice(&room_id);
        out.extend_from_slice(&client_tick.to_be_bytes());
        out.extend_from_slice(&player_actor_id.to_be_bytes());
        out.push(len);
        out.extend_from_slice(&input_payload[..len as usize]);
    }

    /// Build a complete standalone packet `Vec<u8>`.
    pub fn to_packet_vec(
        room_id: [u8; 8],
        client_tick: u32,
        player_actor_id: u16,
        input_payload: &[u8],
    ) -> Vec<u8> {
        let mut pkt = Vec::with_capacity(Self::HEADER_LEN + input_payload.len().min(255));
        Self::encode(room_id, client_tick, player_actor_id, input_payload, &mut pkt);
        pkt
    }

    /// Redundant inputs view: decomposes `input_payload` into `[current, prev_1, prev_2]`
    /// when inputs have a known uniform size.
    pub fn redundant_inputs(&self, single_input_size: usize) -> Option<RedundantInputs<'a>> {
        RedundantInputs::parse_uniform(self.input_payload, single_input_size)
    }
}

/// Redundancy support helper for reference-style lockstep/rollback inputs.
/// Client transmits [current tick input + last 2 previous inputs].
/// Even if 1 UDP packet drops, clients recover previous ticks without stalling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RedundantInputs<'a> {
    /// Current tick input (tick T)
    pub current: &'a [u8],
    /// Previous input 1 (tick T - 1)
    pub prev_1: Option<&'a [u8]>,
    /// Previous input 2 (tick T - 2)
    pub prev_2: Option<&'a [u8]>,
}

/// Match the 8-byte room id carried by an input-hub datagram against the
/// room the sender is actually connected to. The SDK truncates the UTF-8 room
/// name to 8 bytes and zero-pads it. An all-zero id is rejected: it would let
/// one room's packets be replayed into another.
pub fn room_id_matches(room_name: &str, room_id: &[u8; 8]) -> bool {
    if *room_id == [0u8; 8] {
        return false;
    }
    let bytes = room_name.as_bytes();
    let n = bytes.len().min(8);
    if room_id[..n] != bytes[..n] {
        return false;
    }
    room_id[n..].iter().all(|&b| b == 0)
}

/// Derive the actor id the Unity SDK uses for a session. This mirrors
/// `CYGNetwork.DeriveActorNumber`: server-assigned `p{id}_...` sessions use
/// that id directly; other sessions use the low 14 bits of FNV-1a over the
/// UTF-16 code units, plus one. Verifying this stops one joined WebSocket
/// client from claiming another client's actor id.
pub fn derive_input_actor(session_id: &str) -> u16 {
    if session_id.is_empty() {
        return 1;
    }
    if let Some(rest) = session_id.strip_prefix('p') {
        if let Some((digits, _)) = rest.split_once('_') {
            if let Ok(id) = digits.parse::<u32>() {
                if id > 0 && id <= u16::MAX as u32 {
                    return id as u16;
                }
            }
        }
    }
    let mut hash: u32 = 2_166_136_261;
    for unit in session_id.encode_utf16() {
        hash ^= unit as u32;
        hash = hash.wrapping_mul(16_777_619);
    }
    ((hash & 0x3FFF) as u16) + 1
}

/// Small token bucket used to cap input-hub packets per peer/session. It is
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
        Self::new(INPUT_HUB_MAX_PACKET_HZ, INPUT_HUB_RATE_BURST)
    }
}

impl<'a> RedundantInputs<'a> {
    /// Parse uniform-sized redundant inputs from payload.
    ///
    /// - 3 * `single_len` => `current`, `prev_1`, `prev_2`
    /// - 2 * `single_len` => `current`, `prev_1`
    /// - 1 * `single_len` => `current`
    pub fn parse_uniform(payload: &'a [u8], single_len: usize) -> Option<Self> {
        if single_len == 0 {
            return None;
        }
        if payload.len() == single_len * 3 {
            Some(Self {
                current: &payload[0..single_len],
                prev_1: Some(&payload[single_len..single_len * 2]),
                prev_2: Some(&payload[single_len * 2..single_len * 3]),
            })
        } else if payload.len() == single_len * 2 {
            Some(Self {
                current: &payload[0..single_len],
                prev_1: Some(&payload[single_len..single_len * 2]),
                prev_2: None,
            })
        } else if payload.len() == single_len {
            Some(Self {
                current: payload,
                prev_1: None,
                prev_2: None,
            })
        } else {
            None
        }
    }

    /// Encode redundant inputs `[current tick input + last 2 previous inputs]` into a contiguous buffer.
    pub fn encode_redundant(
        current: &[u8],
        prev_1: Option<&[u8]>,
        prev_2: Option<&[u8]>,
        out: &mut Vec<u8>,
    ) {
        out.extend_from_slice(current);
        if let Some(p1) = prev_1 {
            out.extend_from_slice(p1);
            if let Some(p2) = prev_2 {
                out.extend_from_slice(p2);
            }
        }
    }

    /// Build a complete `CYG_WIRE_INPUT_TICK` packet with redundant inputs.
    pub fn encode_redundant_packet(
        room_id: [u8; 8],
        client_tick: u32,
        player_actor_id: u16,
        current: &[u8],
        prev_1: Option<&[u8]>,
        prev_2: Option<&[u8]>,
        out: &mut Vec<u8>,
    ) {
        let mut payload = Vec::new();
        Self::encode_redundant(current, prev_1, prev_2, &mut payload);
        WireInputTick::encode(room_id, client_tick, player_actor_id, &payload, out);
    }
}

/// Tunnel prefix inside ROOM_DATA_BYTES that marks the rest as a canonical CYG
/// packet. The transport strips the `17` header byte, so a game-layer payload
/// reads `[0x43][len_hi][len_lo][canonical_op][body...]`.
pub const CYG_TUNNEL_MAGIC: u8 = 0x43;

/// Split a `ROOM_DATA_BYTES` payload into `(canonical_op, body)`.
///
/// Layout matches the SDK exactly: `[0x43][len_be u16][op][body]`, where `len`
/// counts the op byte plus the body. Returns `None` for any other room payload
/// (msgpack, voice, delta blobs — those are relayed opaquely).
#[inline]
pub fn parse_tunnel(data: &[u8]) -> Option<(u8, &[u8])> {
    if data.len() < 4 || data[0] != CYG_TUNNEL_MAGIC {
        return None;
    }
    let len = u16::from_be_bytes([data[1], data[2]]) as usize;
    if len == 0 {
        return None;
    }
    if data.len() == 4 + len {
        // C# convention where len == body.len()
        Some((data[3], &data[4..4 + len]))
    } else if data.len() >= 3 + len {
        // Rust convention where len == 1 + body.len()
        Some((data[3], &data[4..3 + len]))
    } else {
        None
    }
}

/// Build a `ROOM_DATA_BYTES` payload for one canonical packet. Send it with the
/// wire header [`CYG_WIRE_ROOM_DATA_BYTES`] (17) — the `0x43` magic is the
/// payload's first byte, never a second header.
pub fn write_tunnel(out: &mut Vec<u8>, op: u8, body: &[u8]) {
    out.clear();
    out.push(CYG_TUNNEL_MAGIC);
    out.extend_from_slice(&((body.len() + 1) as u16).to_be_bytes());
    out.push(op);
    out.extend_from_slice(body);
}

/// Decode the signed LEB128 zig-zag varint that `CYGBitPacker::WriteInt32`
/// emits. Returns `(value, bytes_consumed)`.
#[inline]
pub fn read_varint_i32(buf: &[u8]) -> Option<(i32, usize)> {
    let mut shift = 0u32;
    let mut acc = 0u64;
    for (i, &b) in buf.iter().enumerate() {
        acc |= ((b & 0x7F) as u64) << shift;
        if b & 0x80 == 0 {
            let zig = acc as u32;
            let value = ((zig >> 1) as i32) ^ -((zig & 1) as i32);
            return Some((value, i + 1));
        }
        shift += 7;
        if shift > 35 {
            return None;
        }
    }
    None
}

/// Action to take on relayed Unity packet
#[derive(Debug, Clone)]
pub struct UnityRelayAction {
    /// None = broadcast to all others in room (sender excluded)
    /// Some(session) = send point-to-point to specific session
    pub target: Option<String>,
    /// Wire header code peers must see so they deframe it exactly like the
    /// sender did: `17` for a tunneled canonical packet, `15` for a view patch.
    pub wire_code: u8,
    pub wire_payload: Vec<u8>,
}

/// Dispatches a Unity CYG packet or tunneled relay frame.
/// Returns Some(action) if handled as a Unity relay message.
pub fn handle_unity_relay_packet(
    room: &mut GameRoom,
    sender_session: &str,
    data: &[u8],
) -> Option<UnityRelayAction> {
    if data.is_empty() {
        return None;
    }

    // Case 1: tunneled canonical CYG packet. The transport already stripped the
    // ROOM_DATA_BYTES header byte, so the payload is [0x43][len_be][op][body].
    if let Some((canonical_op, body)) = parse_tunnel(data) {
        let target = match canonical_op {
            CYG_OP_CUSTOM_RPC => {
                let mut target = None;
                if body.len() >= 7 {
                    let target_byte = body[6];
                    if target_byte == RPC_TARGET_OWNER || target_byte == 4 {
                        // SDK writes the view id as two big-endian u16 halves.
                        let view_id = u32::from_be_bytes([body[0], body[1], body[2], body[3]]);
                        if let Some(ref unity) = room.unity {
                            if let Some(ent) = unity.entities.get(&view_id) {
                                target = Some(ent.owner_session.clone());
                            } else if let Some((_, owner, _)) = unity.spawned_prefabs.get(&view_id) {
                                // Production relay records ownership at spawn
                                // time; `entities` is only populated when a full
                                // snapshot is restored. Without this fallback an
                                // owner-targeted RPC could never resolve and would
                                // leak to every peer.
                                target = Some(owner.clone());
                            }
                        }
                    } else if target_byte == RPC_TARGET_SERVER {
                        // Server-authoritative RPC: absorbed, nothing to relay.
                        return Some(UnityRelayAction {
                            target: Some(String::new()),
                            wire_code: CYG_WIRE_ROOM_DATA_BYTES,
                            wire_payload: Vec::new(),
                        });
                    }
                }
                target
            }
            CYG_OP_SPAWN_OBJECT => {
                // [zigzag varint view_id][u16 BE prefab_id][owner actor][pose...]
                if let Some((view_id, used)) = read_varint_i32(body) {
                    if body.len() >= used + 2 {
                        let prefab_id = u16::from_be_bytes([body[used], body[used + 1]]);
                        if let Some(ref mut unity) = room.unity {
                            unity.spawned_prefabs.insert(
                                view_id as u32,
                                (prefab_id as u32, sender_session.to_string(), body.to_vec()),
                            );
                        }
                    }
                }
                None
            }
            CYG_OP_DESPAWN_OBJECT => {
                if let Some((view_id, _)) = read_varint_i32(body) {
                    if let Some(ref mut unity) = room.unity {
                        unity.spawned_prefabs.remove(&(view_id as u32));
                        unity.entities.remove(&(view_id as u32));
                    }
                }
                None
            }
            // RewindHit (7) and any future tunneled op relay opaquely.
            _ => None,
        };
        return Some(UnityRelayAction {
            target,
            wire_code: CYG_WIRE_ROOM_DATA_BYTES,
            wire_payload: data.to_vec(),
        });
    }

    // Case 2: client view state patch (wire 15). Payload is
    // [15][zigzag varint view_id][component walk...]. Only the view id is needed
    // to mark the entity dirty; peers get the patch back under its own header.
    if data[0] == CYG_WIRE_ROOM_STATE_PATCH {
        if let Some((view_id, _)) = read_varint_i32(&data[1..]) {
            if let Some(ref mut unity) = room.unity {
                unity.dirty_entities.insert(view_id as u32);
            }
        }
        return Some(UnityRelayAction {
            target: None,
            wire_code: CYG_WIRE_ROOM_STATE_PATCH,
            wire_payload: data[1..].to_vec(),
        });
    }

    // No other top-level wire code is a game-layer message: 18 is PING (echoed
    // by the transport), 19 is the input-hub frame, 21 is ROOM_REQUEST, and
    // 10/12 are the join/leave handshake. Anything unrecognised is dropped
    // rather than misread as a sync packet.
    None
}

/// Mirrors `CYGBitPacker::WriteInt32`: zig-zag LEB128.
pub fn put_varint(out: &mut Vec<u8>, value: i32) {
    let mut v = ((value << 1) ^ (value >> 31)) as u32;
    loop {
        if v < 0x80 {
            out.push(v as u8);
            return;
        }
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rooms::GameRoom;

    /// The exact `ROOM_DATA_BYTES` payload the SDK puts on the wire.
    fn tunnel(op: u8, body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        write_tunnel(&mut out, op, body);
        out
    }

    #[test]
    fn tunnel_layout_is_magic_first_and_round_trips() {
        let pkt = tunnel(CYG_OP_DESPAWN_OBJECT, &[9, 9]);
        assert_eq!(pkt[0], CYG_TUNNEL_MAGIC, "0x43 must be the payload's first byte");
        assert_eq!(u16::from_be_bytes([pkt[1], pkt[2]]), 3, "len counts op + body");
        assert_eq!(pkt[3], CYG_OP_DESPAWN_OBJECT);
        assert_eq!(parse_tunnel(&pkt), Some((CYG_OP_DESPAWN_OBJECT, &[9u8, 9][..])));
    }

    #[test]
    fn varint_matches_sdk_bitpacker() {
        for v in [0i32, 1, -1, 42, 300, -300, 65_535, -65_536, i32::MAX, i32::MIN] {
            let mut buf = Vec::new();
            put_varint(&mut buf, v);
            let (got, used) = read_varint_i32(&buf).expect("decodes");
            assert_eq!(got, v, "value {v}");
            assert_eq!(used, buf.len(), "consumed every byte for {v}");
        }
    }

    #[test]
    fn test_unity_tunneled_rpc_routing() {
        let mut room = GameRoom::new_unity_relay();
        if let Some(ref mut unity) = room.unity {
            unity.update_transform(42, "sess_charlie", [0.0; 3], [0.0, 0.0, 0.0, 1.0], [0.0; 3]);
        }

        // SDK RPC body: [viewId as 2 BE u16][method hash BE][target byte][args].
        let rpc_body = |target: u8| {
            let mut b = Vec::new();
            b.extend_from_slice(&42u32.to_be_bytes());
            b.extend_from_slice(&0x1234u16.to_be_bytes());
            b.push(target);
            b
        };

        let others = tunnel(CYG_OP_CUSTOM_RPC, &rpc_body(RPC_TARGET_OTHERS));
        assert!(handle_unity_relay_packet(&mut room, "sess_alice", &others)
            .expect("handled")
            .target
            .is_none());

        let owner = tunnel(CYG_OP_CUSTOM_RPC, &rpc_body(RPC_TARGET_OWNER));
        assert_eq!(
            handle_unity_relay_packet(&mut room, "sess_alice", &owner)
                .expect("handled")
                .target,
            Some("sess_charlie".to_string())
        );

        let server = tunnel(CYG_OP_CUSTOM_RPC, &rpc_body(RPC_TARGET_SERVER));
        assert_eq!(
            handle_unity_relay_packet(&mut room, "sess_alice", &server)
                .expect("handled")
                .target,
            Some(String::new())
        );
    }

    /// The test above seeds `entities` via `update_transform`, which production
    /// never calls — the live relay only records ownership through the spawn
    /// packet. Owner routing must therefore work off that record, otherwise an
    /// owner-targeted RPC silently broadcasts to every peer.
    #[test]
    fn owner_rpc_routes_to_the_spawner_via_the_spawn_record() {
        let mut room = GameRoom::new_unity_relay();

        // Production path: a spawn packet from sess_owner registers view 42.
        let mut spawn = Vec::new();
        put_varint(&mut spawn, 42);
        spawn.extend_from_slice(&7u16.to_be_bytes());
        spawn.extend_from_slice(&[0u8; 12]); // owner actor + pose (opaque to relay)
        let spawn_pkt = tunnel(CYG_OP_SPAWN_OBJECT, &spawn);
        handle_unity_relay_packet(&mut room, "sess_owner", &spawn_pkt).expect("spawn handled");

        // A different peer sends an owner-targeted RPC for that view id.
        let mut body = Vec::new();
        body.extend_from_slice(&42u32.to_be_bytes());
        body.extend_from_slice(&0x1234u16.to_be_bytes());
        body.push(RPC_TARGET_OWNER);
        let rpc = tunnel(CYG_OP_CUSTOM_RPC, &body);

        assert_eq!(
            handle_unity_relay_packet(&mut room, "sess_other", &rpc)
                .expect("handled")
                .target,
            Some("sess_owner".to_string()),
            "owner-targeted RPC must unicast to the spawner, not broadcast"
        );
    }

    #[test]
    fn test_unity_spawn_and_despawn_relay() {
        let mut room = GameRoom::new_unity_relay();

        // Spawn: [varint viewId=200][u16 BE prefabId=500][tail]
        let mut spawn_body = Vec::new();
        put_varint(&mut spawn_body, 200);
        spawn_body.extend_from_slice(&500u16.to_be_bytes());
        spawn_body.extend_from_slice(&[1, 2, 3, 4]);
        let spawn_packet = tunnel(CYG_OP_SPAWN_OBJECT, &spawn_body);

        let act = handle_unity_relay_packet(&mut room, "sess_spawner", &spawn_packet).expect("handled");
        assert!(act.target.is_none());
        assert_eq!(act.wire_code, CYG_WIRE_ROOM_DATA_BYTES);
        assert_eq!(act.wire_payload, spawn_packet, "peers get the tunnel verbatim");

        let unity = room.unity.as_ref().unwrap();
        assert!(unity.spawned_prefabs.contains_key(&200));
        let (prefab_id, owner, _) = &unity.spawned_prefabs[&200];
        assert_eq!(*prefab_id, 500);
        assert_eq!(owner, "sess_spawner");

        // Despawn: [varint viewId=200]
        let mut despawn_body = Vec::new();
        put_varint(&mut despawn_body, 200);
        let despawn_packet = tunnel(CYG_OP_DESPAWN_OBJECT, &despawn_body);

        assert!(handle_unity_relay_packet(&mut room, "sess_spawner", &despawn_packet)
            .expect("handled")
            .target
            .is_none());
        assert!(!room.unity.as_ref().unwrap().spawned_prefabs.contains_key(&200));
    }

    #[test]
    fn state_patch_marks_dirty_with_varint_view_id() {
        let mut room = GameRoom::new_unity_relay();

        // Wire code 15, then [varint viewId=300][component walk...].
        let mut pkt = vec![CYG_WIRE_ROOM_STATE_PATCH];
        put_varint(&mut pkt, 300);
        pkt.extend_from_slice(&[0x01, 0xAA, 0xBB]);

        let action = handle_unity_relay_packet(&mut room, "sess_mover", &pkt).expect("handled");
        assert!(action.target.is_none());
        assert_eq!(action.wire_code, CYG_WIRE_ROOM_STATE_PATCH);
        assert_eq!(action.wire_payload, pkt[1..], "the 15 header byte is stripped");
        assert!(room.unity.as_ref().unwrap().dirty_entities.contains(&300));
    }

    #[test]
    fn non_tunnel_wire_codes_are_not_misread() {
        let mut room = GameRoom::new_unity_relay();

        // PING (18) is echoed by the transport; it must never become a broadcast.
        assert!(handle_unity_relay_packet(&mut room, "s", &[CYG_WIRE_PING, 1, 2, 3]).is_none());
        // A short input-tick packet is not a relay message. A complete one is
        // consumed by the input-hub path before relay dispatch.
        assert!(handle_unity_relay_packet(&mut room, "s", &[CYG_WIRE_INPUT_TICK, 0, 0, 0, 0, 0, 0, 0]).is_none());
        assert!(handle_unity_relay_packet(&mut room, "s", &[20, 0, 0, 0, 0, 0]).is_none());
        assert!(handle_unity_relay_packet(&mut room, "s", &[CYG_WIRE_ROOM_REQUEST, 0, 0, 0, 0]).is_none());
        // A bare 17 with no magic is an opaque room blob, not a canonical packet.
        assert!(handle_unity_relay_packet(&mut room, "s", &[CYG_WIRE_ROOM_DATA_BYTES, 9, 9]).is_none());
        assert!(parse_tunnel(&[CYG_WIRE_ROOM_DATA_BYTES, CYG_TUNNEL_MAGIC, 0, 1, 6]).is_none());
    }

    /// Native UDP clients (Godot/Unreal OpenOxide) read the room delta as
    /// `[i64 local_id][u32 entity_count][entities...]` after skipping the
    /// optional 6-byte `0xA5 "delta"` tag. Keep the tag offsets pinned here:
    /// a change silently turns every entity id/position into garbage.
    #[test]
    fn udp_delta_frame_matches_native_clients() {
        let mut out = Vec::new();
        let delta = [1u8, 0, 0, 0]; // entity_count = 1, one 72-byte record follows
        wrap_delta_into(&mut out, 42, &delta, 7, 99);

        assert_eq!(out[0], 0xA5);
        assert_eq!(&out[1..6], b"delta");
        assert_eq!(i64::from_le_bytes(out[6..14].try_into().unwrap()), 42);
        assert_eq!(u32::from_le_bytes(out[14..18].try_into().unwrap()), 1);

        let tail = out.len() - 6;
        assert_eq!(u16::from_le_bytes(out[tail..tail + 2].try_into().unwrap()), 7);
        assert_eq!(u32::from_le_bytes(out[tail + 2..].try_into().unwrap()), 99);
    }

    #[test]
    fn test_wire_input_tick_parse_and_encode() {
        let room_id = *b"room1234";
        let client_tick = 1042;
        let player_actor_id = 7;
        let payload = [0xAA, 0xBB, 0xCC, 0xDD];

        let pkt = WireInputTick::to_packet_vec(room_id, client_tick, player_actor_id, &payload);
        assert_eq!(pkt.len(), 16 + 4);
        assert_eq!(pkt[0], CYG_WIRE_INPUT_TICK);
        assert_eq!(&pkt[1..9], b"room1234");
        assert_eq!(u32::from_be_bytes(pkt[9..13].try_into().unwrap()), 1042);
        assert_eq!(u16::from_be_bytes(pkt[13..15].try_into().unwrap()), 7);
        assert_eq!(pkt[15], 4);
        assert_eq!(&pkt[16..20], &[0xAA, 0xBB, 0xCC, 0xDD]);

        let parsed = WireInputTick::parse(&pkt).expect("valid input tick");
        assert_eq!(parsed.room_id, room_id);
        assert_eq!(parsed.client_tick, 1042);
        assert_eq!(parsed.player_actor_id, 7);
        assert_eq!(parsed.input_len, 4);
        assert_eq!(parsed.input_payload, &[0xAA, 0xBB, 0xCC, 0xDD]);
        assert_eq!(parsed.room_id_str().unwrap(), "room1234");
        assert!(!parsed.has_valid_input_payload(), "4-byte payload is not a Unity input frame");
        assert_ne!(CYG_WIRE_INPUT_TICK, CYG_WIRE_PING, "input tick must never collide with PING");
    }

    #[test]
    fn input_hub_accepts_only_unity_input_frames() {
        let room_id = *b"room1234";
        for (len, ok) in [(0usize, false), (4, false), (9, true), (18, true), (27, true), (36, false)] {
            let payload = vec![0u8; len];
            let pkt = WireInputTick::to_packet_vec(room_id, 1, 1, &payload);
            let parsed = WireInputTick::parse(&pkt).expect("parse");
            assert_eq!(parsed.has_valid_input_payload(), ok, "payload len {len}");
        }
    }

    #[test]
    fn input_hub_room_id_is_scoped_to_the_connected_room() {
        let mut id = [0u8; 8];
        id[..7].copy_from_slice(b"my-game");
        assert!(room_id_matches("my-game", &id));
        assert!(!room_id_matches("other", &id));
        assert!(!room_id_matches("my-game", &[0u8; 8]));
        // Names longer than 8 bytes are truncated by the SDK.
        let mut long_id = [0u8; 8];
        long_id.copy_from_slice(b"my-game-");
        assert!(room_id_matches("my-game-extra", &long_id));
        assert!(!room_id_matches("other-room", &long_id));
    }

    #[test]
    fn input_hub_actor_matches_the_unity_derivation() {
        // Mirrors CYGNetwork.DeriveActorNumber.
        // Values cross-checked against the C# implementation.
        assert_eq!(derive_input_actor("hello"), 11_436);
        assert_eq!(derive_input_actor("p7_127.0.0.1_9000"), 7);
        assert_eq!(derive_input_actor("sdk_1720000000000_42"), 7_598);
        assert_eq!(
            derive_input_actor("0f8fad5b-d9cb-469f-a165-70867728950e"),
            5_826
        );
        assert_ne!(derive_input_actor("p0_ignored"), 0);
        assert_eq!(derive_input_actor(""), 1);
        for session in ["a", "sess-1", "0f8fad5b-d9cb-469f-a165-70867728950e"] {
            let actor = derive_input_actor(session);
            assert!((1..=16_384).contains(&actor));
        }
    }

    #[test]
    fn input_hub_rate_limiter_rejects_a_flood() {
        let mut limiter = InputRateLimiter::new(10, 2.0);
        let now = Instant::now();
        assert!(limiter.allow(now));
        assert!(limiter.allow(now));
        assert!(!limiter.allow(now), "burst exhausted");
        assert!(limiter.allow(now + std::time::Duration::from_millis(250)));
    }

    #[test]
    fn test_wire_input_tick_bounds() {
        // Less than 16 bytes: rejected
        let short = [CYG_WIRE_INPUT_TICK, 1, 2, 3];
        assert!(WireInputTick::parse(&short).is_none());

        // Wrong opcode: rejected
        let mut invalid_op = vec![0u8; 16];
        invalid_op[0] = 99;
        assert!(WireInputTick::parse(&invalid_op).is_none());

        // Header claims 10 bytes payload, but buffer has 0: rejected
        let mut truncated = vec![0u8; 16];
        truncated[0] = CYG_WIRE_INPUT_TICK;
        truncated[15] = 10;
        assert!(WireInputTick::parse(&truncated).is_none());

        // Exact framing: junk appended after the declared payload is rejected
        // rather than forwarded to every peer.
        let mut trailing = WireInputTick::to_packet_vec([1u8; 8], 1, 1, &[0u8; 9]);
        trailing.extend_from_slice(b"junk");
        assert!(WireInputTick::parse(&trailing).is_none());
    }

    #[test]
    fn test_wire_input_tick_redundancy() {
        let room_id = [0u8; 8];
        let tick = 500;
        let actor = 1;

        let input_t = [1u8, 2, 3, 4];
        let input_t_minus_1 = [5u8, 6, 7, 8];
        let input_t_minus_2 = [9u8, 10, 11, 12];

        let mut pkt = Vec::new();
        RedundantInputs::encode_redundant_packet(
            room_id,
            tick,
            actor,
            &input_t,
            Some(&input_t_minus_1),
            Some(&input_t_minus_2),
            &mut pkt,
        );

        let parsed = WireInputTick::parse(&pkt).expect("parse redundant packet");
        assert_eq!(parsed.input_len, 12);

        let redundant = parsed.redundant_inputs(4).expect("parse uniform redundant");
        assert_eq!(redundant.current, &[1, 2, 3, 4]);
        assert_eq!(redundant.prev_1, Some(&[5u8, 6, 7, 8] as &[u8]));
        assert_eq!(redundant.prev_2, Some(&[9u8, 10, 11, 12] as &[u8]));
    }
}
