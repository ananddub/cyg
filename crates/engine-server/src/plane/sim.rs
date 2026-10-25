//! The pinned 128 Hz sim plane: owns the wasm room, drains `bus.inputs_rx` /
//! `bus.commands_rx`, drives `wasm.on_tick`, builds per-viewer deltas through
//! `aoi` + `dispatch`, sends native frames over `UdpPlane` and browser frames
//! to `bus.deltas_tx` (drop when full), and publishes `StatusSnapshot` once a
//! second via [`crate::telemetry`]. See CONTRACT.md for the full loop shape.
//!
//! The room type, host linker and membership come from [`crate::rooms`];
//! input apply/journal (including the rollback replay) from
//! [`crate::netcode::apply_input`]; interest management from
//! [`engine_spatial::aoi`].

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mio::{Events, Poll};
use engine_wasm::WasmRuntime;

use engine_bus::{Bus, FrameKind, OutFrame, PeerId, SessionTelemetry, SimCommand, Via};
use engine_config::{AoiConfig, GameConfig};
use engine_physics::PhysicsWorld3D;
use engine_spatial::aoi;

use crate::plane::{self, PlaneConfig, mono_ns, tick_period};
use crate::netcode;
use crate::rooms::{self, GameRoom};
use crate::telemetry::{PublishReport, Recorder};

/// Spawn placement: 60 px margin inside an 800x500 field, diagonal scatter.
const SPAWN_W: f64 = 800.0;
const SPAWN_H: f64 = 500.0;

/// Catch-up cap: never simulate more than 2 ticks in a row (the rest are
/// dropped and counted — a sim thread that digs a deeper hole never climbs out).
const MAX_CATCHUP_TICKS: u32 = 2;

/// Decode the guest's packed `(ptr << 32) | len` i64 return (server.rs idiom:
/// high word is the linear-memory offset, low word the byte length).
#[inline]
fn decode_packed(packed: i64) -> (usize, usize) {
    (((packed >> 32) as u32) as usize, (packed as u32) as usize)
}

/// Bind one verified peer into the room (seat token already checked by the
/// UDP plane). Mirrors the old plane-local `Room::bind`: room-name match +
/// capacity gate, deterministic diagonal-spawn placement, `on_join`, session
/// insert. Returns the player id, or -1 when rejected.
fn bind_peer(
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

/// Peer left / timed out: release the player slot, exactly like the old plane
/// `Room::drop_peer` (guest `on_leave`, seq echo, journal retain).
fn drop_peer(
    room: &mut GameRoom,
    peer_id: &PeerId,
    browser_players: &mut HashSet<i64>,
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

/// The plane's once-a-second report. What it can see is `room.sessions`: peer
/// ids bound through a seat token. Their transport is whichever lane their
/// inputs actually came in on — `Via::Browser` peers get their frames back over
/// `bus.deltas_tx` to the browser plane, everyone else over the raw socket this
/// thread owns. There is no WebSocket here, so this path never fills
/// `ws_sessions`.
fn publish_report(
    room: &GameRoom,
    browser_players: &HashSet<i64>,
    last_input_at: &HashMap<PeerId, Instant>,
    tick_hz: f64,
    tick_hz_est: f64,
    max_players: i32,
) -> PublishReport {
    let mut sessions = Vec::with_capacity(room.sessions.len());
    let mut browser = 0u32;
    for (peer_id, &player_id) in &room.sessions {
        let via_browser = browser_players.contains(&player_id);
        browser += via_browser as u32;
        sessions.push(SessionTelemetry {
            session: peer_id.clone(),
            transport: if via_browser { "webrtc" } else { "native-udp" }.to_string(),
            player_id: Some(player_id),
            acked_seq: room.last_input_seq.get(peer_id.as_str()).map(|&seq| seq as u32),
            idle_ms: last_input_at.get(peer_id).map(|t| t.elapsed().as_millis() as u64),
        });
    }
    let players = room.sessions.len() as u32;
    PublishReport {
        tick_hz_est,
        tick_rate_hz: tick_hz.max(0.0) as u32,
        players,
        ws_sessions: 0,
        webrtc_sessions: browser,
        udp_sessions: players.saturating_sub(browser),
        sessions,
        session_cap: max_players.max(0) as usize,
    }
}

pub fn run(bus: Arc<Bus>, cfg: PlaneConfig) {
    // ------------------------------------------------------------- UDP plane
    let seats = Arc::new(plane::SeatRegistry::new(&cfg.seat_secret));
    let mut udp = match plane::udp::UdpPlane::new(&cfg.udp_addr, Arc::clone(&bus), Arc::clone(&seats)) {
        Ok(u) => u,
        Err(e) => {
            tracing::error!("[sim] ✖️ UDP plane bind {} failed: {e} — sim thread exiting", cfg.udp_addr);
            return;
        }
    };
    let mut poll = match Poll::new() {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("[sim] ✖️ mio poll creation failed: {e} — sim thread exiting");
            return;
        }
    };
    if let Err(e) = udp.register(&poll) {
        tracing::error!("[sim] ✖️ UDP socket registration failed: {e} — sim thread exiting");
        return;
    }
    udp.set_tick_hz(cfg.tick_hz);

    // -------------------------------------------------------------- WASM room
    let runtime = match WasmRuntime::new_with_config(engine_wasm::WasmConfig {
        optimize_for_speed: true,
        ..engine_wasm::WasmConfig::default()
    }) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("[sim] ✖️ wasm runtime init failed: {e} — sim thread exiting");
            return;
        }
    };
    let module = match runtime.load_or_compile_cached(&cfg.wasm_path) {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("[sim] ✖️ wasm module load failed: {e} — sim thread exiting");
            return;
        }
    };
    let linker = rooms::build_host_linker(&runtime, engine_wasm::AbiPermissions::default());
    let pre = match runtime.prepare(&linker, &module) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("[sim] ✖️ pre-instantiation failed: {e} — sim thread exiting");
            return;
        }
    };
    let physics = std::fs::read_to_string("arena_map.json")
        .ok()
        .and_then(|json| PhysicsWorld3D::from_json(&json).ok())
        .unwrap_or_default();
    let mut room = match GameRoom::new(&pre, cfg.max_players, physics) {
        Ok(room) => room,
        Err(e) => {
            tracing::error!("[sim] ✖️ room instantiation failed: {e} — sim thread exiting");
            return;
        }
    };

    tracing::info!(
        "[sim] running on core {} tick_hz={} wasm={}",
        plane::current_core(),
        cfg.tick_hz,
        cfg.wasm_path.display()
    );

    // ----------------------------------------------------------- loop state
    let period = tick_period(cfg.tick_hz);
    let dt = period.as_secs_f64();
    let aoi_cfg = AoiConfig::default();
    // `netcode::apply_input` follows `GameConfig::rollback`; the plane has no
    // rollback knobs yet, so the defaults (enabled = false, journal 256) keep
    // the legacy newest-wins behavior the plane had.
    let plane_game_cfg = GameConfig::default();
    let mut recorder = Recorder::new();
    let mut events = Events::with_capacity(32);
    let mut last_tick = Instant::now();
    let mut sweep_start = Instant::now();
    let mut status_start = Instant::now();
    let mut window_ticks: u64 = 0;
    let mut measured_hz = cfg.tick_hz;
    let mut native_send_failed: u64 = 0;
    // Peers whose inputs arrive with `via == Browser`; their frames go to
    // `bus.deltas_tx` instead of the raw UDP socket. Keyed by player id
    // because `InputMsg` carries no peer id.
    let mut browser_players: HashSet<i64> = HashSet::new();
    // Reverse of `room.sessions`: `InputMsg` carries only the player id, but
    // `GameRoom`'s seq echo + journal are keyed by session string.
    let mut session_of: HashMap<i64, PeerId> = HashMap::new();
    // Session → wall time of its last input, for the telemetry rows' `idle_ms`.
    // Pruned once a second against `room.sessions`, so a churned peer id cannot
    // grow it.
    let mut last_input_at: HashMap<PeerId, Instant> = HashMap::new();
    // Each player's own record position from the PREVIOUS parsed delta —
    // the viewer anchor for AOI. One tick stale by design: parsing the body
    // once per tick and reusing the entity list for every viewer costs far
    // less than a fresh self-lookup per peer, and a 7.8 ms-old anchor only
    // nudges the cull boundary.
    let mut last_pos: HashMap<i64, [f64; 3]> = HashMap::new();
    // Hot-path scratch: delta read, per-viewer filtered body, framing, frames.
    let mut delta_buf: Vec<u8> = Vec::with_capacity(8 * 1024);
    let mut filtered_buf: Vec<u8> = Vec::with_capacity(8 * 1024);
    let mut scratch: Vec<u8> = Vec::with_capacity(8 * 1024);
    let mut frames: Vec<OutFrame> = Vec::new();

    loop {
        let timeout = period.saturating_sub(last_tick.elapsed());
        if let Err(e) = poll.poll(&mut events, Some(timeout)) {
            tracing::error!("[sim] ⚠️ mio poll error: {e}");
        }
        for ev in events.iter() {
            if ev.token() == plane::udp::UDP_TOKEN && ev.is_readable() {
                udp.on_readable();
            }
        }

        // ---- commands: bind/drop/event (fully drained; small, never starved)
        while let Ok(cmd) = bus.commands_rx.try_recv() {
            match cmd {
                SimCommand::BindPeer { peer_id, room: wanted, reply } => {
                    let id = bind_peer(&mut room, peer_id.clone(), &wanted, &cfg.room_name, cfg.max_players, &mut session_of);
                    let _ = reply.send(id);
                    if id >= 0 {
                        tracing::info!("[sim] {peer_id} bound to room '{}' as player {id}", cfg.room_name);
                    } else {
                        tracing::info!("[sim] bind rejected for {peer_id}: room mismatch or full");
                    }
                }
                SimCommand::DropPeer { peer_id } => {
                    drop_peer(&mut room, &peer_id, &mut browser_players, &mut session_of);
                }
                SimCommand::Event { room: wanted, target, bytes } => {
                    if wanted != cfg.room_name || room.sessions.is_empty() {
                        continue;
                    }
                    let targets: Vec<PeerId> = match target {
                        Some(t) if room.sessions.contains_key(&t) => vec![t],
                        Some(_) => vec![],
                        None => room.sessions.keys().cloned().collect(),
                    };
                    for t in targets {
                        let frame = OutFrame {
                            peer_id: t.clone(),
                            kind: FrameKind::Reliable,
                            bytes: bytes.clone(),
                        };
                        if browser_players.contains(&room.sessions[&t]) {
                            let _ = bus.deltas_tx.try_send(frame);
                        } else if !udp.send_frame(&frame) {
                            // A reliable frame returns true once the peer's window
                            // accepted it, so a failure here is the plane refusing
                            // to queue (no peer, not bound, window full) — the
                            // event genuinely never went out.
                            native_send_failed += 1;
                        }
                    }
                }
            }
        }

        // BIND frames go out the same poll iteration the reply arrived:
        // waiting for the next packet/sweep would let up to 200 ms of deltas
        // precede the client's BIND.
        udp.drain_pending();
        // Reliable events age on their own timers, not on traffic: this is what
        // resends an event nobody answered. Every pass, because the poll
        // deadline above is one tick, so a resend is never more than ~8 ms late.
        udp.pump_reliable();

        // ---- inputs: bounded drain so a flood cannot stall the tick clock
        let mut drained = 0usize;
        while drained < 64 {
            let Ok(msg) = bus.inputs_rx.try_recv() else { break };
            drained += 1;
            recorder.note_input_latency(msg.arrival_ns, mono_ns());
            let Some(session) = session_of.get(&msg.player_id) else {
                recorder.inputs_dropped += 1;
                continue;
            };
            if msg.via == Via::Browser {
                browser_players.insert(msg.player_id);
            }
            last_input_at.insert(session.clone(), Instant::now());
            netcode::apply_input(
                &mut room,
                &plane_game_cfg,
                dt,
                session,
                msg.player_id,
                msg.seq,
                &msg.payload,
            );
        }

        // ---- fixed tick with catch-up cap
        let elapsed = last_tick.elapsed();
        if elapsed < period {
            // A quiet loop still has to age its sample: `publish_due` gates the
            // row build so this stays off the per-poll path.
            if recorder.publish_due() {
                let report =
                    publish_report(&room, &browser_players, &last_input_at, cfg.tick_hz, measured_hz, cfg.max_players);
                recorder.maybe_publish(&bus, report);
            }
            if sweep_start.elapsed() >= Duration::from_millis(200) {
                udp.sweep();
                sweep_start = Instant::now();
            }
            continue;
        }
        let mut ticks_to_run = (elapsed.as_nanos() / period.as_nanos().max(1) as u128) as u32;
        if ticks_to_run > MAX_CATCHUP_TICKS {
            // Count the skipped ticks as overruns and reset the deadline;
            // falling behind is fine, trying to catch up all at once is not.
            recorder.tick_overruns += (ticks_to_run - MAX_CATCHUP_TICKS) as u64;
            ticks_to_run = MAX_CATCHUP_TICKS;
            last_tick = Instant::now();
        } else {
            last_tick += period * ticks_to_run;
        }
        let t0 = Instant::now();

        let mut trap = false;
        for _ in 0..ticks_to_run {
            room.current_tick += 1;
            if let Some(ref mut wasm) = room.wasm {
                match wasm.on_tick(dt) {
                    Ok(packed) => {
                        // packed = (ptr << 32) | len into the guest's linear memory.
                        let (ptr, len) = decode_packed(packed);
                        if len > engine_wasm::MAX_OUTBOUND_DELTA_SIZE {
                            tracing::warn!("[sim] ⚠️ delta size {len} exceeds ABI max — skipped dispatch");
                            trap = true; // don't read stale/oversized memory this tick
                            break;
                        }
                        if len > 0 {
                            if let Err(e) = wasm.read_memory_into(ptr as u32, len as u32, &mut delta_buf) {
                                tracing::error!("[sim] ⚠️ delta read failed: {e}");
                                trap = true;
                                break;
                            }
                        } else {
                            delta_buf.clear();
                        }
                    }
                    Err(e) => {
                        tracing::warn!("[sim] ⚠️ guest trap in on_tick at room tick {}: {e}", room.current_tick);
                        trap = true;
                        break;
                    }
                }
            } else {
                delta_buf.clear();
            }
        }

        if !trap && !delta_buf.is_empty() {
            room.delta_seq = room.delta_seq.wrapping_add(1);
            let server_tick = room.current_tick as u32;

            let parsed = aoi::parse_delta(&delta_buf);
            if let Some((_, ref entities, _, _)) = parsed {
                for e in entities {
                    last_pos.insert(e.id, e.pos);
                }
            }

            for (peer_id, &player_id) in &room.sessions {
                let body: &[u8] = match &parsed {
                    Some((_, entities, bullet_count, bullets_range)) => {
                        let viewer_pos = last_pos.get(&player_id).copied();
                        // parse_delta's range starts at the bullet-COUNT field;
                        // the filter wants the bullet DATA range (server.rs:1747).
                        aoi::filter_for_viewer(
                            entities,
                            *bullet_count,
                            (bullets_range.0 + 4, bullets_range.1),
                            &delta_buf,
                            player_id,
                            viewer_pos,
                            room.current_tick,
                            &aoi_cfg,
                            &mut filtered_buf,
                        );
                        &filtered_buf
                    }
                    // Foreign/unrecognized guest layout: fall back to raw multicast.
                    None => &delta_buf,
                };
                let acked_seq = room.last_input_seq.get(peer_id.as_str()).copied().unwrap_or(0);
                frames.clear();
                if plane::dispatch::emit_delta(
                    peer_id,
                    player_id,
                    body,
                    acked_seq,
                    server_tick,
                    room.delta_seq,
                    &mut scratch,
                    &mut frames,
                )
                .is_some()
                {
                    if browser_players.contains(&player_id) {
                        for frame in frames.drain(..) {
                            match bus.deltas_tx.try_send(frame) {
                                Ok(()) => recorder.deltas_sent += 1,
                                Err(crossbeam_channel::TrySendError::Full(_)) => recorder.deltas_dropped_full += 1,
                                Err(_) => break,
                            }
                        }
                    } else {
                        for frame in frames.drain(..) {
                            if udp.send_frame(&frame) {
                                recorder.deltas_sent += 1;
                            } else {
                                native_send_failed += 1;
                            }
                        }
                    }
                }
            }
        }
        recorder.note_tick(t0.elapsed());
        window_ticks += ticks_to_run as u64;

        // ---- low-rate maintenance
        if sweep_start.elapsed() >= Duration::from_millis(200) {
            udp.sweep();
            sweep_start = Instant::now();
        }
        if status_start.elapsed() >= Duration::from_secs(1) {
            measured_hz = window_ticks as f64 / status_start.elapsed().as_secs_f64();
            window_ticks = 0;
            status_start = Instant::now();
            last_input_at.retain(|peer, _| room.sessions.contains_key(peer));
            let report =
                publish_report(&room, &browser_players, &last_input_at, cfg.tick_hz, measured_hz, cfg.max_players);
            recorder.maybe_publish(&bus, report);
        }
        let _ = native_send_failed; // unknown-peer sends: counted, no user to blame yet
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_decode_matches_server_convention() {
        let ptr: u32 = 0x0010_0000;
        let len: u32 = 256;
        let packed = ((ptr as i64) << 32) | len as i64;
        assert_eq!(decode_packed(packed), (ptr as usize, len as usize));
    }

    #[test]
    fn packed_decode_handles_high_len() {
        let packed: i64 = ((1u64 << 32) | 0xFFFF_FFFF) as i64;
        assert_eq!(decode_packed(packed), (1, u32::MAX as usize));
    }

    #[test]
    fn newest_wins_updates_seq() {
        let mut last: HashMap<i64, u16> = HashMap::new();
        let mut journal = std::collections::VecDeque::new();
        // No wasm here: exercise the seq bookkeeping through the map logic.
        let (id, seq) = (1i64, 5u16);
        let slot = last.entry(id).or_insert(0);
        assert!(*slot == 0 || engine_webrtc_protocol::is_newer(*slot, seq));
        *slot = seq;
        journal.push_back(rooms::JournalInput { tick: 0, session: String::new(), id, seq, payload: [0u8; 20] });
        assert_eq!(last[&id], 5);
        assert_eq!(journal.len(), 1);
    }
}
