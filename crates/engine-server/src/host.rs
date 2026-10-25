//! Host run loop: the fixed-rate tick + per-session delta broadcast (AOI
//! dispatch).
//!
//! This loop is the default game path, so it is also the telemetry source: it
//! owns the sessions and the tick, and publishes a `StatusSnapshot` onto
//! `bus.status_tx` once a second for the API's `/status` and dashboard.
//! Without that the dashboard would only ever see the pinned `OX_PLANES=1` sim
//! thread, which most projects never run.
//!
//! The room type, linker and membership live in [`crate::rooms`]; the byte
//! pipeline (delta wrapping, AOI filtering, input journaling) in
//! [`crate::netcode`].

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use engine_bus::{Bus, SessionTelemetry};
use engine_net::framing::OpCode;
use engine_net::transport::webrtc::contract::Inbound;
use engine_net::transport::{GameTransport, TransportEvent, WebRtcServer, WebRtcTransport};
use engine_physics::PhysicsWorld3D;
use engine_wasm::WasmRuntime;

use crate::netcode;
use crate::rooms::{self, GameRoom, GameServer};
use crate::telemetry::{PublishReport, Recorder};

/// Pre-framed spectator "you are watching" confirm: `["delta"]` with id 0.
const SPECTATOR_CONFIRM: &[u8] = &[0xA5, b'd', b'e', b'l', b't', b'a', 0, 0, 0, 0, 0, 0, 0, 0];

/// Bound the inbound frames handled per loop iteration. The transport drains a
/// socket fully on readiness (hundreds of frames per poll), and net + sim now
/// share a thread: without this cap one burst delays the fixed tick until the
/// whole batch is relayed.
const MAX_FRAMES_PER_LOOP: usize = 256;

/// OpenOxide app-message opcode (client <-> server, typed).
///
/// Wire shape: `[0x05][type_len u8][UTF-8 type][opaque payload]`.
/// This is our own envelope, deliberately independent of Colyseus/msgpack.
/// `sendInput` (0x01), mesh events (0x03), voice (0x56), signals (0x57) and the
/// spectator handshake (0xFF) keep their existing frames.
const APP_MESSAGE_OPCODE: u8 = 0x05;

/// The game loop's view of the WebRTC DataChannel transport.
///
/// The transport runs on the caller's thread: `poll_events` services the UDP
/// socket and the str0m sessions, `send_to` queues straight into a session's
/// lane and `flush` writes once per loop iteration, so there is no outbound
/// channel and no burst can be lost to a full `try_send` queue.
///
/// Seat bookkeeping lives here, not in the wire transport: a WebRTC session is
/// born from a signaling offer whose seat names the room, so the loop mirrors
/// seat -> room/options at offer time and consults those maps on connect.
pub struct LoopTransport {
    rtc: WebRtcTransport,
    /// session id -> room id, mirrored from the seat at signaling time so the
    /// loop can route connect/disconnect events without borrowing the wire.
    client_rooms: Arc<scc::HashMap<String, String>>,
    /// session id -> raw join options JSON carried by the seat's matchmake
    /// call (empty until matchmake starts persisting them).
    client_options: HashMap<String, String>,
    /// Sessions whose disconnect was intentional (a server-initiated close).
    /// Drained by `take_consented`.
    consented: std::collections::HashSet<String>,
    /// Abnormally-disconnected sessions kept alive for reconnection:
    /// reconnectionToken ("roomId:sessionId") -> session id.
    retained: HashMap<String, String>,
    /// Lane-classified inbound frames drained from the wire transport during
    /// `poll_events`; the loop takes them once per iteration.
    inbound: Vec<(String, Inbound)>,
    local_addr: Option<SocketAddr>,
}

impl LoopTransport {
    fn new(rtc: WebRtcTransport) -> Self {
        let local_addr = rtc.local_addr().ok();
        Self {
            rtc,
            client_rooms: Arc::new(scc::HashMap::new()),
            client_options: HashMap::new(),
            consented: std::collections::HashSet::new(),
            retained: HashMap::new(),
            inbound: Vec::new(),
            local_addr,
        }
    }

    /// Record a seat's routing before (or as) its WebRTC session comes up:
    /// which room it joins and what options its matchmake call carried.
    pub fn register_seat(&mut self, session_id: &str, room: &str, options: &str) {
        let _ = self
            .client_rooms
            .insert_sync(session_id.to_string(), room.to_string());
        self.client_options
            .insert(session_id.to_string(), options.to_string());
    }

    pub fn room_of(&self, client_id: &str) -> Option<String> {
        self.client_rooms.read_sync(client_id, |_, r| r.clone())
    }

    /// Raw JSON join options the seat's matchmake call carried.
    pub fn options_of(&self, client_id: &str) -> Option<String> {
        self.client_options.get(client_id).cloned()
    }

    /// Whether the session's disconnect was intentional (a server-initiated
    /// close). Such seats skip `allowReconnection` holds.
    pub fn take_consented(&mut self, client_id: &str) -> bool {
        self.consented.remove(client_id)
    }

    /// Keep this session's reconnection token valid so the SDK can resume it
    /// with `POST /matchmake/reconnect/<room>`.
    pub fn retain_session(&mut self, room_id: &str, session_id: &str) {
        self.retained.insert(
            format!("{}:{}", room_id, session_id),
            session_id.to_string(),
        );
    }

    pub fn take_room_of(&mut self, client_id: &str) -> Option<String> {
        let room = self.client_rooms.remove_sync(client_id).map(|(_, r)| r);
        if room.is_some() {
            self.client_options.remove(client_id);
        }
        room
    }

    /// Frames dropped because a peer stopped draining its lanes.
    pub fn dropped_frames(&self) -> u64 {
        self.rtc.dropped_frames()
    }

    /// The wire-level WebRTC server: signaling re-offers, the lane-classified
    /// inbound drain, stamped delta sends and reliable broadcasts.
    pub fn rtc_server(&mut self) -> &mut WebRtcServer {
        self.rtc.server_mut()
    }

    /// Read-only WebRTC session probe (telemetry).
    pub fn is_rtc_connected(&self, session: &str) -> bool {
        self.rtc.server().is_connected(session)
    }

    /// Lane-classified inbound frames drained by the last `poll_events`.
    pub fn take_inbound(&mut self) -> Vec<(String, Inbound)> {
        std::mem::take(&mut self.inbound)
    }

    /// Stamp and fragment one world delta onto a session's unreliable lane.
    /// Returns the frame count; zero means the caller should fall back to the
    /// reliable lane (`send_state`/`send_to`).
    pub fn send_delta_framed(
        &mut self,
        session: &str,
        seq: u16,
        tick: u32,
        body: &[u8],
    ) -> usize {
        netcode::send_delta_framed(self.rtc.server_mut(), session, seq, tick, body)
    }

    /// Queue a schema state frame on the latest-wins lane (unflushed state
    /// frames are replaced, not appended).
    pub fn send_state(&mut self, client_id: &str, payload: &[u8]) -> std::io::Result<()> {
        self.rtc.send_state(client_id, payload)
    }

    pub fn send_to_many(
        &mut self,
        client_ids: &[&str],
        opcode: OpCode,
        payload: &[u8],
    ) -> std::io::Result<()> {
        for &cid in client_ids {
            let _ = self.rtc.send_to(cid, opcode, payload);
        }
        Ok(())
    }
}

impl GameTransport for LoopTransport {
    fn poll_events(
        &mut self,
        timeout: Duration,
        events: &mut Vec<TransportEvent>,
    ) -> std::io::Result<()> {
        self.rtc.poll_events(timeout, events)?;
        // The wire transport also derives `Packet { PlayerInput }` events from
        // the contract's input lane; this loop consumes the lane-classified
        // stream instead (it needs seq/resupply/ack context the derived event
        // drops), so the derived view is discarded here.
        events.retain(|ev| {
            !matches!(ev, TransportEvent::Packet { opcode: OpCode::PlayerInput, .. })
        });
        self.inbound.extend(self.rtc.poll_inbound());
        Ok(())
    }

    fn send_to(&mut self, client_id: &str, opcode: OpCode, payload: &[u8]) -> std::io::Result<()> {
        self.rtc.send_to(client_id, opcode, payload)
    }

    fn broadcast(&mut self, opcode: OpCode, payload: &[u8]) -> std::io::Result<()> {
        self.rtc.broadcast(opcode, payload)
    }

    fn disconnect(&mut self, client_id: &str) -> std::io::Result<()> {
        self.rtc.disconnect(client_id)
    }

    fn disconnect_with(&mut self, client_id: &str, _code: u16, _reason: &str) -> std::io::Result<()> {
        // There is no close frame on a DataChannel: a server-initiated close
        // just tears the session down, and its consent marker decides whether
        // the seat is held for reconnection.
        self.consented.insert(client_id.to_string());
        self.rtc.disconnect(client_id)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.rtc.flush()
    }

    fn ccu(&self) -> usize {
        self.rtc.ccu()
    }

    fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.local_addr.ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::AddrNotAvailable, "no local addr")
        })
    }

    fn transport_name(&self) -> &'static str {
        "webrtc-datagram-loop"
    }
}

/// Extracts the base room name without query string or stray session params.
fn sanitize_room_name(raw: &str) -> &str {
    let s = raw.split('?').next().unwrap_or(raw);
    s.split("sessionId=").next().unwrap_or(s)
}

/// Strip the msgpack type header from an inbound ROOM_DATA_BYTES payload:
/// payload = [msgpack str][data...] -> returns the data part.
fn strip_type_header(payload: &[u8]) -> Option<&[u8]> {
    let first = *payload.first()?;
    if (0xA0..=0xBF).contains(&first) {
        let type_len = (first & 0x1F) as usize;
        payload.get(1 + type_len..)
    } else if first == 0xD9 {
        let type_len = *payload.get(1)? as usize;
        payload.get(2 + type_len..)
    } else {
        // Direct raw binary input from OpenOxide SDK without msgpack overhead
        Some(payload)
    }
}

/// Monotonic milliseconds — the clock the RUDP lane runs on (matches the
/// plane's `mono_ns` convention).
fn mono_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Queue an arbitrary payload on a UDP peer's reliable lane (assigned seq,
/// Go-Back-N resend until the client ACKs). Called from the room-outbox fan-out.
#[allow(dead_code)]
fn udp_send_reliable(
    udp_sock: &std::net::UdpSocket,
    udp_reliable: &mut HashMap<SocketAddr, crate::plane::reliable::ReliableLane>,
    udp_reliable_seq: &mut HashMap<SocketAddr, u16>,
    addr: SocketAddr,
    payload: &[u8],
    scratch: &mut Vec<u8>,
) {
    let seq = udp_reliable_seq
        .entry(addr)
        .and_modify(|s| *s = s.wrapping_add(1))
        .or_insert(1);
    let seq = *seq;
    netcode::encode_reliable_msg(seq, payload, scratch);
    let wire = scratch.clone();
    if let Some(lane) = udp_reliable.get_mut(&addr) {
        if lane.queue(seq, wire, mono_ms()).is_ok() {
            lane.pump(mono_ms(), |frame| udp_sock.send_to(frame, addr).is_ok());
        }
    }
}

/// Frame and route everything the guest queued through `host_send`.
///
/// The guest names a player id (or 0 for broadcast); the host maps that to
/// transport session ids here. Messages are queued and drained after the tick,
/// so a guest can never write to a socket from inside a Wasm call.
fn flush_room_outbox(room: &mut GameRoom, transport: &mut impl GameTransport) {
    let messages = room.drain_outbox();
    if messages.is_empty() {
        return;
    }

    let mut frame = Vec::with_capacity(64);
    for message in messages {
        if message.event.is_empty() || message.event.len() > 255 {
            continue;
        }
        frame.clear();
        frame.push(APP_MESSAGE_OPCODE);
        frame.push(message.event.len() as u8);
        frame.extend_from_slice(message.event.as_bytes());
        frame.extend_from_slice(&message.payload);

        if message.target > 0 {
            if let Some(session) = room
                .sessions
                .iter()
                .find_map(|(session, id)| (*id == message.target).then_some(session.as_str()))
            {
                let _ = transport.send_to(session, OpCode::RoomDataBytes, &frame);
            }
        } else if message.target == 0 {
            for session in room.sessions.keys() {
                let _ = transport.send_to(session, OpCode::RoomDataBytes, &frame);
            }
            for spectator in &room.spectators {
                let _ = transport.send_to(spectator, OpCode::RoomDataBytes, &frame);
            }
        }
    }
}

/// Tell every voice peer in the mesh, except `subject`, that the roster changed.
///
/// The body is a one-element JSON array so the SDK can read it without the
/// server owning a JSON type. One frame is encoded and shared across the fan-out.
fn announce_voice_peer_change(
    transport: &mut impl GameTransport,
    mesh: &mut engine_voice::VoiceMesh,
    kind: engine_voice::SignalKind,
    subject: u32,
) {
    let frame = engine_voice::SignalFrame::new(
        kind,
        engine_voice::SERVER_SENDER,
        engine_voice::BROADCAST,
        bytes::Bytes::from(format!("[{subject}]")),
    )
    .encode();
    mesh.for_each_peer(subject, |target| {
        let _ = transport.send_to(target.session_id, OpCode::RoomDataBytes, &frame);
    });
}

/// Reject a signaling frame a client is not allowed to send. `to` is the sender:
/// these replies only ever go back to the offender, who then tears down that peer.
fn reject_signal(
    transport: &mut impl GameTransport,
    client_id: &str,
    from: u32,
    reason: &'static str,
) {
    let frame = engine_voice::SignalFrame::new(
        engine_voice::SignalKind::Error,
        engine_voice::SERVER_SENDER,
        from,
        bytes::Bytes::from(format!(r#"{{"reason":"{reason}"}}"#)),
    )
    .encode();
    let _ = transport.send_to(client_id, OpCode::RoomDataBytes, &frame);
}

/// Answer a rejected join: ERROR(4002) with a JSON body, then a `WITH_ERROR`
/// close, in one flush. `WITH_ERROR` is terminal for the SDKs, so they surface
/// the reason instead of reconnecting forever.
fn reject_room(transport: &mut LoopTransport, client_id: &str, message: &str) {
    let code = engine_net::close_code::WITH_ERROR;
    let reason = format!("{{\"code\":{code},\"message\":\"{message}\"}}");
    let _ = transport.send_to(client_id, OpCode::Error, reason.as_bytes());
    let _ = transport.disconnect_with(client_id, code, message);
    let _ = transport.flush();
}

/// Run the server until killed.
///
/// `bus` is the same `Arc<Bus>` the control plane was spawned with: this loop is
/// its only `status_tx` producer on the default path.
pub fn run(
    project_dir: &Path,
    wasm_path: &Path,
    cfg: &engine_config::GameConfig,
    bus: Arc<Bus>,
    signals: Option<engine_api::infra::signaling::SignalHub>,
) -> std::io::Result<()> {
    // ------------------------------------------------------------------
    // WASM runtime — the room logic module (hot-swappable).
    // ------------------------------------------------------------------
    let runtime = WasmRuntime::new_with_config(engine_wasm::WasmConfig {
        optimize_for_speed: true,
        ..engine_wasm::WasmConfig::default()
    })
    .expect("wasm runtime");
    // The room module bytes are only needed for the boot log; the actual load
    // happens below and is allowed to fail into Unity-relay-only mode. A missing
    // file here must not take the whole process down.
    match std::fs::metadata(wasm_path) {
        Ok(m) => tracing::info!(
            "[server] room logic loaded ({} bytes) — generic byte pipeline active",
            m.len()
        ),
        Err(_) => tracing::info!(
            "[server] no room logic file at {} — Unity-relay rooms only",
            wasm_path.display()
        ),
    }

    // The WebRTC game data plane: one UDP socket the str0m sessions share,
    // bound to the CONFIGURED game data port (`port` in config.toml — what the
    // user writes is what binds; the seat reservation advertises the same).
    // Signaling rides the control plane's `/ws/signal` hub; offers handed over
    // here mint the WebRTC sessions.
    let webrtc_port: u16 = cfg.port;
    let webrtc_udp = std::net::UdpSocket::bind(format!("0.0.0.0:{webrtc_port}"))
        .or_else(|_| std::net::UdpSocket::bind("0.0.0.0:0"))
        .ok();
    let mut raw_transport = if let Some(sock) = webrtc_udp {
        let actual_port = sock.local_addr().map(|a| a.port()).unwrap_or(webrtc_port);
        tracing::info!(
            "[server] ⚡ WebRTC DataChannel server listening on udp://0.0.0.0:{actual_port}"
        );
        let local_addr: SocketAddr = format!("127.0.0.1:{actual_port}").parse().unwrap();
        let mut announced = vec![local_addr];
        let lan_ip = std::net::UdpSocket::bind("0.0.0.0:0")
            .and_then(|s| s.connect("8.8.8.8:80").map(|_| s.local_addr()))
            .ok()
            .and_then(|a| a.ok())
            .map(|a| a.ip());
        if let Some(ip) = lan_ip {
            let lan_addr = SocketAddr::new(ip, actual_port);
            if lan_addr != local_addr {
                announced.push(lan_addr);
                tracing::info!("[server] ⚡ WebRTC announced LAN candidate: {lan_addr}");
            }
        }
        if let Ok(ip) = std::env::var("PUBLIC_IP")
            && let Ok(a) = format!("{ip}:{actual_port}").parse()
        {
            announced.push(a);
        }
        WebRtcTransport::from_socket(sock, announced).expect("webrtc transport")
    } else {
        tracing::error!("[server] ⚠️ webrtc bind :{webrtc_port} failed — WebRTC data plane disabled");
        WebRtcTransport::from_socket(
            std::net::UdpSocket::bind("127.0.0.1:0").expect("ephemeral udp bind"),
            Vec::new(),
        )
        .expect("ephemeral webrtc transport")
    };

    // Prefer the cooked OXCL arena (ox-game cook <map.glb> arena.oxcl): it
    // feeds both the WASM host-data physics template and the native ECS rooms.
    let oxcl_path = project_dir.join("arena.oxcl");
    let arena_mesh: Option<engine_physics::oxcl::CookedMesh> =
        match std::fs::read(&oxcl_path) {
            Ok(bytes) => match engine_physics::oxcl::CookedMesh::from_oxcl_bytes(&bytes) {
                Ok(mesh) => {
                    tracing::info!(
                        "[server] 🗺️  loaded arena.oxcl ({} verts, {} indices)",
                        mesh.vertices.len(),
                        mesh.indices.len()
                    );
                    Some(mesh)
                }
                Err(e) => {
                    tracing::error!("[server] failed to parse arena.oxcl: {e}");
                    None
                }
            },
            Err(_) => None,
        };

    let map_path = project_dir.join("arena_map.json");
    let arena_2d_path = project_dir.join("arena_2d.json");

    let arena_2d: Option<engine_physics::MapConfig2D> = if arena_2d_path.exists() {
        match std::fs::read_to_string(&arena_2d_path) {
            Ok(json_str) => match serde_json::from_str::<engine_physics::MapConfig2D>(&json_str) {
                Ok(m) => {
                    tracing::info!(
                        "[server] 🗺️  loaded arena_2d.json with {} static colliders ({}x{})",
                        m.obstacles.len(),
                        m.arena_width,
                        m.arena_height
                    );
                    Some(m)
                }
                Err(e) => {
                    tracing::error!("[server] failed to parse arena_2d.json: {e}");
                    None
                }
            },
            Err(_) => None,
        }
    } else if map_path.exists() {
        match std::fs::read_to_string(&map_path) {
            Ok(json_str) => match serde_json::from_str::<engine_physics::MapConfig2D>(&json_str) {
                Ok(m) => {
                    tracing::info!(
                        "[server] 🗺️  loaded arena_map.json as 2D map with {} static colliders ({}x{})",
                        m.obstacles.len(),
                        m.arena_width,
                        m.arena_height
                    );
                    Some(m)
                }
                Err(_) => None,
            },
            Err(_) => None,
        }
    } else {
        None
    };

    let physics = if map_path.exists() && arena_2d.is_none() {
        match std::fs::read_to_string(&map_path) {
            Ok(json_str) => match PhysicsWorld3D::from_json(&json_str) {
                Ok(w) => {
                    tracing::info!(
                        "[server] 🗺️  loaded arena_map.json with {} static colliders",
                        w.obstacles.len()
                    );
                    w
                }
                Err(e) => {
                    tracing::error!("[server] failed to parse arena_map.json: {e}, using default world");
                    PhysicsWorld3D::default()
                }
            },
            Err(_) => PhysicsWorld3D::default(),
        }
    } else {
        PhysicsWorld3D::default()
    };

    // Generated by `ox-game run` from schema.ts. Clients receive it on their
    // reliable DataChannel lane as soon as it opens.
    // Descriptor lives in out/ (conventional layout); older projects keep it
    // at the root. Both are valid.
    let candidates = [
        project_dir.join("out").join("openoxide.schema.bin"),
        project_dir.join("openoxide.schema.bin"),
    ];
    let found = candidates.iter().find(|p| p.exists());
    let schema_descriptor = found
        .and_then(|p| std::fs::read(p).ok())
        .unwrap_or_default();
    if !schema_descriptor.is_empty() {
        tracing::info!(
            "[server] 📋 schema descriptor loaded ({} bytes) from {}",
            schema_descriptor.len(),
            found
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "unknown".into())
        );
        raw_transport.set_schema_descriptor(schema_descriptor.clone());
    }

    // Host-side schema views: the same descriptor clients receive also tells
    // the host how to walk entity rows, so AOI needs no guest cooperation.
    let schema_aoi = if cfg.aoi.enabled && !schema_descriptor.is_empty() {
        let parsed = crate::schema_view::SchemaAoi::from_descriptor(&schema_descriptor);
        if parsed.is_none() {
            tracing::warn!(
                "[server] ⚠️ [aoi] enabled but the schema has no x/y fields — schema state stays global"
            );
        }
        parsed
    } else {
        None
    };

    // Direct UDP Listener for Unity / Unreal / Godot native clients. The bind
    // address follows the same `OX_UDP_ADDR` knob as the pinned sim plane, so a
    // second instance (or a container) can move it off the default :7778.
    // Bound before the transport enters the loop so the default seat
    // reservation advertises the port that was actually opened.
    let udp_addr = std::env::var("OX_UDP_ADDR").unwrap_or_else(|_| "0.0.0.0:7778".into());
    let udp_socket = std::net::UdpSocket::bind(&udp_addr).ok();
    if let Some(ref s) = udp_socket {
        let _ = s.set_nonblocking(true);
        match s.local_addr() {
            Ok(addr) => {
                tracing::info!(
                    "[server] listening on udp://{addr} (Unity / Unreal / Godot Native Engine)"
                );
            }
            Err(_) => tracing::info!(
                "[server] listening on udp://{udp_addr} (Unity / Unreal / Godot Native Engine)"
            ),
        }
    } else {
        tracing::error!("[server] ⚠️ udp bind {udp_addr} failed — native UDP clients disabled");
    }

    // One thread: transport I/O and the simulation share this loop. An idle
    // wait below blocks inside `poll`, so network work is still serviced while
    // the sim waits for its next tick.
    let mut transport = LoopTransport::new(raw_transport);
    let mut udp_peers: HashMap<SocketAddr, (i64, String, Instant)> = HashMap::new();
    // RUDP: per-peer reliable send lane (server -> client, per-frame resend
    // timers until the client's cumulative ack) + the next outgoing seq per
    // peer, and a received-seq ring for inbound dedupe (QUIC-style: every
    // frame is delivered on arrival, reordering is fine — only resend
    // duplicates are suppressed, nothing ever blocks anything).
    let mut udp_reliable: HashMap<SocketAddr, crate::plane::reliable::ReliableLane> = HashMap::new();
    let mut udp_reliable_seq: HashMap<SocketAddr, u16> = HashMap::new();
    let mut udp_rxd_seq: HashMap<SocketAddr, std::collections::HashSet<u16>> = HashMap::new();
    const RUDP_DEDUPE_SPAN: usize = 256;
    let mut rudp_scratch: Vec<u8> = Vec::with_capacity(2100);
    // Signaling hand-over: the control plane's hub queues browser offers; this
    // loop is the data-plane owner and drains them once per iteration.
    let mut signal_rx = signals.as_ref().and_then(|hub| hub.take_signals());
    // Input-hub flood control: one token bucket per transport identity. These
    // are created lazily and dropped with the session/peer.
    let mut rtc_input_rates: HashMap<String, netcode::InputRateLimiter> = HashMap::new();
    let mut udp_input_rates: HashMap<SocketAddr, netcode::InputRateLimiter> = HashMap::new();
    // Join-path bucket: creating a player runs guest `on_join` AND replies
    // with the full schema descriptor — a spoofed-source flood must not mint
    // players or amplify traffic. Reuses the same limiter type (5/s refill).
    let mut udp_join_rates: HashMap<SocketAddr, netcode::InputRateLimiter> = HashMap::new();

    tracing::info!(
        "[server] ABI contract: host v{} (manifest requested: v{})",
        engine_wasm::CURRENT_ABI_VERSION,
        cfg.abi_version
    );
    if cfg.abi_version > engine_wasm::CURRENT_ABI_VERSION {
        tracing::warn!(
            "[server] ⚠️  Warning: manifest requests ABI version {}, but server only supports up to v{}",
            cfg.abi_version,
            engine_wasm::CURRENT_ABI_VERSION
        );
    }

    let wasm_load_start = Instant::now();
    // Link + pre-instantiate once; every room spawn after this is one
    // `InstancePre::instantiate` instead of rebuilding 12 host imports.
    //
    // A missing or broken room module is not fatal: a project that ships only
    // Unity C# gameplay has no `room.ts`, and the server still has to boot and
    // serve Unity-relay rooms. We log it and carry on with `pre = None`.
    let pre = runtime
        .load_or_compile_cached(wasm_path)
        .and_then(|module| {
            let linker = rooms::build_host_linker(&runtime, cfg.permissions);
            runtime.prepare(&linker, &module)
        })
        .map_err(|e| tracing::info!("[server] load room module: {e}"))
        .ok();
    let wasm_available = pre.is_some();
    if wasm_available {
        let is_aot = wasm_path.with_extension("cwasm").exists();
        tracing::info!(
            "[server] ⚡ room module loaded in {:.2?} (mode: {})",
            wasm_load_start.elapsed(),
            if is_aot {
                "AOT precompiled (.cwasm)"
            } else {
                "JIT compiled (.wasm)"
            }
        );
    } else {
        tracing::info!(
            "[server] 🧩 no room module — running Unity-relay rooms only (WASM rooms disabled)"
        );
    }
    let mut server = GameServer::new(pre, cfg.max_clients, physics, cfg.permissions);
    server.arena_2d = arena_2d;

    let mut last_tick = Instant::now();
    let tick_interval = Duration::from_secs_f64(1.0 / cfg.tick_rate.max(1) as f64);
    let mut tick_counter: u64 = 0;
    let mut tick_stat_start = Instant::now();
    let mut last_wasm_check = Instant::now();
    let mut loaded_wasm_time: Option<std::time::SystemTime> = if wasm_available {
        std::fs::metadata(wasm_path)
            .ok()
            .and_then(|m| m.modified().ok())
    } else {
        None
    };
    let source_path = project_dir.join(&cfg.source);
    let mut loaded_src_time: Option<std::time::SystemTime> = std::fs::metadata(&source_path)
        .ok()
        .and_then(|m| m.modified().ok());
    let mut events = Vec::new();
    // Tick scratch: the delta read and the framed broadcast reuse these.
    let mut delta_buf: Vec<u8> = Vec::with_capacity(8 * 1024);
    // Per-viewer AOI output; reused across viewers (the caller copies each
    // body into the outgoing delta before the next viewer).
    let mut view_buf: Vec<u8> = Vec::with_capacity(8 * 1024);
    let mut frame_buf: Vec<u8> = Vec::with_capacity(8 * 1024);
    // The WebRTC path writes one stamped frame per chunk, so the delta body needs
    // a buffer of its own; `frame_buf` holds only the current chunk.
    let mut delta_body: Vec<u8> = Vec::with_capacity(8 * 1024);
    let mut filtered_buf: Vec<u8> = Vec::with_capacity(8 * 1024);
    let mut dead_rooms: Vec<String> = Vec::new();
    let mut timed_out_udp: Vec<(SocketAddr, i64, String)> = Vec::new();
    // Telemetry: the recorder gates itself to one publish per second, and the
    // rows are built only when that window actually rolls, never per packet.
    let mut recorder = Recorder::new();
    let mut publish_start = Instant::now();
    let mut window_ticks: u64 = 0;
    // Native-UDP fan-out batch: one reusable buffer so the per-tick delta
    // broadcast costs one sendmmsg instead of one sendto per peer.
    let mut udp_batch: Vec<(SocketAddr, Vec<u8>)> = Vec::new();
    // Session → wall time its last inbound input was handed to the guest. This
    // is the only place `idle_ms` can come from: the transport keeps no
    // per-session arrival clock. Entries are dropped with the session itself.
    let mut last_input_at: HashMap<String, Instant> = HashMap::new();

    loop {
        // ---- signaling hand-over: browser SDP -> str0m sessions ----
        // Offers arrive on the control plane's `/ws/signal` socket; the seat
        // names the room, so the seat is mirrored here before the session it
        // mints can surface a `Connected` event. Answers go back through the
        // same hub, to whatever socket is still attached for that seat.
        if let (Some(hub), Some(rx)) = (signals.as_ref(), signal_rx.as_mut()) {
            while let Ok(sig) = rx.try_recv() {
                match sig {
                    engine_api::infra::signaling::SignalIn::Offer { seat, sdp } => {
                        transport.register_seat(&seat.id, &seat.room, "");
                        match transport.rtc_server().handle_offer(&seat.id, &sdp) {
                            Ok(answer) => {
                                hub.publish(
                                    &seat.id,
                                    engine_api::infra::signaling::SignalOut::Answer {
                                        sdp: answer,
                                    },
                                );
                                tracing::info!(
                                    "[server] ⚡ WebRTC offer accepted for seat {} (room '{}'), SDP answer returned",
                                    seat.id, seat.room
                                );
                            }
                            Err(e) => {
                                hub.publish(
                                    &seat.id,
                                    engine_api::infra::signaling::SignalOut::Error {
                                        message: e,
                                    },
                                );
                            }
                        }
                    }
                    engine_api::infra::signaling::SignalIn::Candidate { seat, candidate, .. } => {
                        if let Err(e) = transport.rtc_server().handle_candidate(&seat.id, &candidate) {
                            tracing::error!(
                                "[server] ⚠️ seat {} candidate error: {e} ({candidate})",
                                seat.id
                            );
                        }
                    }
                    // The signaling socket went away. Clean up the transport session immediately
                    // so the seat is released and `on_leave` runs without waiting for idle timeout.
                    engine_api::infra::signaling::SignalIn::Close { seat } => {
                        tracing::info!("[server] signaling socket closed for seat {}", seat.id);
                        let _ = transport.disconnect_with(&seat.id, 1000, "signaling socket closed");
                    }
                }
            }
        }

        // One wait per iteration, inside the poll: sockets are serviced at full
        // speed and the loop wakes at the next tick deadline. The blocking poll
        // must happen here rather than at the bottom of the loop: the transport
        // hands its events to `events`, and they are drained immediately below.
        // A capped batch may already be queued: drain it before reading more,
        // otherwise a sustained burst grows the backlog faster than it drains.
        if events.is_empty() {
            let remaining_to_tick = tick_interval.saturating_sub(last_tick.elapsed());
            let all_idle =
                server.rooms.values().all(|r| r.is_hibernating()) && udp_peers.is_empty();
            let wait = if all_idle {
                remaining_to_tick.min(Duration::from_millis(5))
            } else {
                remaining_to_tick
            };
            transport.poll_events(wait, &mut events).expect("poll");
        }

        let batch = events.len().min(MAX_FRAMES_PER_LOOP);
        for event in events.drain(..batch) {
            match event {
                TransportEvent::Connected { client_id, .. } => {
                    let raw_room_str = transport.room_of(&client_id);
                    let raw_room = raw_room_str.as_deref().unwrap_or(&cfg.name);
                    let is_spectator = raw_room.contains("spectator");
                    // Native bevy_ecs rooms: room names containing "ecs" run the
                    // Rust-native 7-pillar simulation (no WASM guest).
                    let is_ecs = raw_room.contains("ecs");
                    let room_name = sanitize_room_name(raw_room);

                    // DoS guard: an unauthenticated client can name any room, so
                    // cap how many distinct rooms an open name space can allocate
                    // (each may be a WASM instance). Live rooms keep working.
                    if server.would_exceed_room_limit(room_name, cfg.max_rooms) {
                        tracing::error!(
                            "[server] ⛔ room limit reached ({}/{}): rejecting {client_id} for '{room_name}'",
                            server.rooms.len(),
                            cfg.max_rooms
                        );
                        reject_room(&mut transport, &client_id, "room limit reached");
                        continue;
                    }

                    let room = if is_ecs {
                        server.get_or_create_ecs(room_name, arena_mesh.as_ref())
                    } else {
                        match server.get_or_create(room_name) {
                            Ok(room) => room,
                            Err(e) => {
                                // A guest that predates the ABI (or a broken
                                // module) must cost one client, never the process.
                                tracing::error!(
                                    "[server] ⛔ room '{room_name}' unavailable: {e} — rejecting {client_id}"
                                );
                                reject_room(&mut transport, &client_id, "room unavailable");
                                continue;
                            }
                        }
                    };
                    room.empty_since = None;
                    let join_options = transport.options_of(&client_id).unwrap_or_default();

                    if is_spectator {
                        room.spectators.insert(client_id.to_string());
                        tracing::info!(
                            "[server] 👁️  {client_id} connected as SPECTATOR to room '{room_name}'"
                        );
                    } else if room.sessions.contains_key(&*client_id) {
                        // Already joined this room: ignore duplicate transport connection events
                        continue;
                    } else if let Some(id) = room.rejoin(&client_id) {
                        // The seat was held by `allowReconnection`: keep the
                        // player id and guest state, skip auth/join.
                        tracing::info!(
                            "[server] 🔁 {client_id} reconnected to room '{room_name}' (player id {id})"
                        );
                        flush_room_outbox(room, &mut transport);
                    } else if room.is_full(cfg.max_clients) {
                        // WASM rooms may override the ceiling via their
                        // `on_is_full` export; relay rooms (no guest) fall back to
                        // the host's `max_clients` so they cannot silently overfill.
                        tracing::error!(
                            "[server] ⛔ room '{room_name}' is full ({}/{}): rejecting {client_id}",
                            room.sessions.len(),
                            cfg.max_clients
                        );
                        reject_room(&mut transport, &client_id, "room is full");
                        continue;
                    } else if !room.authorize(room.next_player_id, &join_options) {
                        // Guest `onAuth` declined before a player id or state
                        // entry existed, so nothing needs unwinding.
                        tracing::error!(
                            "[server] ⛔ room '{room_name}' rejected {client_id} in onAuth"
                        );
                        reject_room(&mut transport, &client_id, "not authorized");
                        continue;
                    } else {
                        let (x, y) = {
                            let n = room.sessions.len();
                            (
                                60.0 + ((n as f64) * 97.0) % (cfg.width - 120.0),
                                60.0 + ((n as f64) * 61.0) % (cfg.height - 120.0),
                            )
                        };
                        let id = room.join(&client_id, x, y);
                        tracing::info!(
                            "[server] {client_id} joined room '{room_name}' (player id {id})"
                        );
                        // Deliver onJoin() messages in this same loop, not one
                        // tick later, while the client is still connected.
                        flush_room_outbox(room, &mut transport);
                    }
                    let _ = transport.flush();
                }
                TransportEvent::Disconnected { client_id } => {
                    tracing::info!("[server] disconnected: {client_id}");
                    last_input_at.remove(&*client_id);
                    rtc_input_rates.remove(&*client_id);
                    let raw_room_str = transport.take_room_of(&client_id);
                    let room_name = if let Some(ref r) = raw_room_str {
                        sanitize_room_name(r).to_string()
                    } else {
                        // Resilient fallback: locate which room holds this session
                        let mut found = None;
                        for (rname, r) in &server.rooms {
                            if r.sessions.contains_key(&*client_id)
                                || r.spectators.contains(&*client_id)
                            {
                                found = Some(rname.clone());
                                break;
                            }
                        }
                        found.unwrap_or_else(|| cfg.name.clone())
                    };
                    // Keep the token valid for the SDK's auto-reconnect. The
                    // transport's retained map is keyed by `room:session` and
                    // is consulted by `matchmake/reconnect`.
                    transport.retain_session(&room_name, &client_id);
                    let Some(room) = server.rooms.get_mut(&room_name) else {
                        continue;
                    };
                    // A WebRTC peer that vanishes with its socket has to be
                    // announced, otherwise the rest of the mesh keeps
                    // re-negotiating with an id nobody answers.
                    // Intentional leaves release the seat immediately; only
                    // unexpected drops may be held for `allowReconnection`.
                    let consented = transport.take_consented(&client_id);
                    let gone = if consented {
                        room.handle_disconnect(&client_id)
                    } else {
                        room.handle_unexpected_disconnect(&client_id)
                    };
                    if let Some(gone) = gone {
                        announce_voice_peer_change(
                            &mut transport,
                            &mut room.voice_mesh,
                            engine_voice::SignalKind::PeerLeft,
                            gone,
                        );
                    }
                    // onLeave() may have queued a roster/chat message.
                    flush_room_outbox(room, &mut transport);
                    transport.rtc_server().remove_session(&client_id);
                    if room.is_hibernating() {
                        tracing::info!(
                            "[server] ⏸️ Room '{room_name}' is now empty -> hibernating (0% CPU, 30s auto-close timer started)"
                        );
                    }
                }
                TransportEvent::Packet {
                    client_id, payload, ..
                } => {
                    // Raw bytes pass-through: strip the msgpack type header,
                    // copy into wasm memory, hand over the pointer.
                    let Some(data) = strip_type_header(&payload) else {
                        continue;
                    };
                    // Rule 4: Bounds validation on inbound payload
                    if data.len() > engine_wasm::MAX_INBOUND_PAYLOAD_SIZE {
                        tracing::warn!(
                            "[server] ⚠️ rejected payload from {client_id}: size {} exceeds max {}",
                            data.len(),
                            engine_wasm::MAX_INBOUND_PAYLOAD_SIZE
                        );
                        continue;
                    }
                    let raw_room_str = transport.room_of(&client_id);
                    let raw_room = raw_room_str.as_deref().unwrap_or(&cfg.name);
                    let room_name = sanitize_room_name(raw_room);
                    let Some(room) = server.rooms.get_mut(room_name) else {
                        continue;
                    };
                    room.note_activity();

                    // Spectator Upgrade Handshake: [0xFF, 'S', 'P', 'E', 'C']
                    if data.len() == 5 && data[0] == 0xFF && &data[1..5] == b"SPEC" {
                        room.make_spectator(&client_id);
                        tracing::info!(
                            "[server] 👁️  {client_id} upgraded to SPECTATOR in room '{room_name}'"
                        );
                        let _ =
                            transport.send_to(&client_id, OpCode::RoomDataBytes, SPECTATOR_CONFIRM);
                        continue;
                    }

                    // Schema readiness ack: the host keeps sending the full row
                    // until the client confirms it applied one. Without this,
                    // a patch racing the full snapshot over a different
                    // transport (WS vs WebRTC) could arrive first and leave
                    // the client without the baseline entities.
                    if data.len() >= 2 && data[0] == 0x05 {
                        let type_len = data[1] as usize;
                        if type_len > 0
                            && 2 + type_len <= data.len()
                            && &data[2..2 + type_len] == b"__ox_schema_ready"
                        {
                            room.schema_sent.insert(client_id.to_string());
                            continue;
                        }
                    }

                    let Some(&id) = room.sessions.get(&*client_id) else {
                        continue;
                    };

                    // WebRTC signaling: ['W'][version][kind][from u32][to u32][JSON]
                    // The host never opens the SDP/ICE body. It stamps the sender
                    // from its own session map, checks the target is a live peer,
                    // and forwards the frame — the media itself goes peer-to-peer.
                    if engine_voice::SignalFrame::is_signal(data) {
                        // Zero-copy view of the bytes the frame actually arrived in.
                        let wire = payload.slice(payload.len() - data.len()..);
                        let Ok(signal) = engine_voice::SignalFrame::decode(&wire) else {
                            tracing::warn!("[server] ⚠️ dropped malformed signal from {client_id}");
                            continue;
                        };
                        let from = id as u32;

                        match signal.kind {
                            engine_voice::SignalKind::Join => {
                                // The joiner gets the ICE list, its own authoritative
                                // id and the peers it can reach; everyone else gets
                                // told a peer appeared. `self` is what lets the client
                                // apply the lower-id-creates-the-offer rule without
                                // guessing its id from replicated state.
                                let mut body = String::from(r#"{"iceServers":"#);
                                body.push_str(&cfg.voice_ice_servers);
                                body.push_str(r#","peers":["#);
                                let peers = room.voice_mesh.peers_sorted();
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
                                let _ =
                                    transport.send_to(&client_id, OpCode::RoomDataBytes, &hello);

                                if room.voice_mesh.join(from, &client_id) {
                                    announce_voice_peer_change(
                                        &mut transport,
                                        &mut room.voice_mesh,
                                        engine_voice::SignalKind::PeerJoined,
                                        from,
                                    );
                                }
                            }
                            engine_voice::SignalKind::Bye => {
                                if room.voice_mesh.leave(from).is_some() {
                                    announce_voice_peer_change(
                                        &mut transport,
                                        &mut room.voice_mesh,
                                        engine_voice::SignalKind::PeerLeft,
                                        from,
                                    );
                                }
                            }
                            _kind if signal.to == engine_voice::SERVER_SENDER => {
                                {
                                    let webrtc = transport.rtc_server();
                                    match signal.kind {
                                        engine_voice::SignalKind::Offer => {
                                            let sdp_str = if let Ok(v) =
                                                serde_json::from_slice::<serde_json::Value>(
                                                    &signal.payload,
                                                ) {
                                                v.get("sdp")
                                                    .and_then(|s| s.as_str())
                                                    .map(|s| s.to_string())
                                            } else {
                                                std::str::from_utf8(&signal.payload)
                                                    .ok()
                                                    .map(|s| s.to_string())
                                            };
                                            if let Some(sdp) = sdp_str {
                                                match webrtc.handle_offer(&client_id, &sdp) {
                                                    Ok(answer_sdp) => {
                                                        let answer_body = serde_json::json!({
                                                            "sdp": answer_sdp,
                                                            "type": "answer"
                                                        })
                                                        .to_string();
                                                        let ans_frame =
                                                            engine_voice::SignalFrame::new(
                                                                engine_voice::SignalKind::Answer,
                                                                engine_voice::SERVER_SENDER,
                                                                from,
                                                                bytes::Bytes::from(answer_body),
                                                            )
                                                            .encode();
                                                        let _ = transport.send_to(
                                                            &client_id,
                                                            OpCode::RoomDataBytes,
                                                            &ans_frame,
                                                        );
                                                        tracing::info!(
                                                            "[server] ⚡ WebRTC Client-Server offer accepted for {client_id}, SDP answer returned"
                                                        );
                                                    }
                                                    Err(e) => {
                                                        tracing::error!(
                                                            "[server] ⚠️ WebRTC handle_offer error for client {client_id}: {e}"
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                        engine_voice::SignalKind::Ice => {
                                            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(
                                                &signal.payload,
                                            ) {
                                                if let Some(cand) =
                                                    v.get("candidate").and_then(|c| c.as_str())
                                                {
                                                    match webrtc.handle_candidate(&client_id, cand)
                                                    {
                                                        Ok(_) => tracing::info!(
                                                            "[server] 🧊 Client {client_id} added ICE candidate: {cand}"
                                                        ),
                                                        Err(e) => tracing::error!(
                                                            "[server] ⚠️ Client {client_id} candidate error: {e} ({cand})"
                                                        ),
                                                    }
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                continue;
                            }
                            kind if kind.is_relayed() => {
                                // Point-to-point only: relaying a broadcast offer
                                // would amplify one inbound packet into the room.
                                if signal.to == engine_voice::BROADCAST
                                    || signal.to == from
                                    || !room.voice_mesh.contains(signal.to)
                                {
                                    reject_signal(
                                        &mut transport,
                                        &client_id,
                                        from,
                                        "unknown voice peer target",
                                    );
                                    continue;
                                }
                                let out = engine_voice::rewrite_sender(&wire, from);
                                room.voice_mesh.route(from, signal.to, |target| {
                                    let _ = transport.send_to(
                                        target.session_id,
                                        OpCode::RoomDataBytes,
                                        &out,
                                    );
                                });
                            }
                            other => {
                                tracing::warn!(
                                    "[server] ⚠️ {client_id} sent a server-only signal ({other:?})"
                                );
                            }
                        }
                        continue;
                    }

                    // Voice Packet Dispatch: [0x56, ...]
                    if data.len() >= engine_voice::HEADER_LEN
                        && data[0] == engine_voice::VOICE_MAGIC
                    {
                        let bytes = bytes::Bytes::copy_from_slice(data);
                        if let Ok(mut packet) = engine_voice::VoicePacket::decode(&bytes) {
                            packet.speaker_id = id as u32;
                            let routed = room.voice_room.dispatch_packet(&packet, Instant::now());
                            let mut wire_packet = data.to_vec();
                            wire_packet[1..5].copy_from_slice(&(id as u32).to_be_bytes());
                            for frame in routed {
                                for (cid, &pid) in &room.sessions {
                                    if pid == id {
                                        continue; // NEVER echo voice packets back to the originating speaker
                                    }
                                    if pid as u32 == frame.listener_id {
                                        let _ = transport.send_to(
                                            cid,
                                            OpCode::RoomDataBytes,
                                            &wire_packet,
                                        );
                                    }
                                }
                            }
                        }
                        continue;
                    }

                    // Voice Settings (Team / Mute / Deafen): [0x56, 'T', team_id], [0x56, 'M', muted], [0x56, 'D', deafened]
                    if data.len() == 3 && data[0] == engine_voice::VOICE_MAGIC {
                        if data[1] == b'T' {
                            let team_id = data[2] as u16;
                            room.voice_room.set_team(id as u32, Some(team_id));
                            tracing::info!("[server] 🛡️ Player {id} switched to Squad {team_id}");
                        } else if data[1] == b'M' {
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
                                    let _ = transport.send_to(
                                        target.session_id,
                                        OpCode::RoomDataBytes,
                                        &note,
                                    );
                                });
                            }
                        } else if data[1] == b'D' {
                            let deafened = data[2] == 1;
                            room.voice_room.set_deafened(id as u32, deafened);
                            tracing::info!("[server] 🎧 Player {id} deafened={deafened}");
                        }
                        continue;
                    }

                    // Delivery ack for a reliable frame: [0x06, seq_u16, kind_u8] (4 bytes).
                    // Checked before the guest pass-through so a 4-byte ack never
                    // reaches `on_input` as if it were game input.
                    if let Some(ack) = engine_webrtc_protocol::parse_ack(data) {
                        netcode::record_channel_ack(room, &client_id, ack);
                        continue;
                    }

                    // QUIC-Style Ordered Event Frame: [0x03, seq_u16, kind_u8, from_u16, target_u16, tick_u16, value_f32] (14 bytes)
                    if data.len() == engine_webrtc_protocol::EVENT_WIRE_LEN
                        && data[0] == engine_webrtc_protocol::OP_EVENT
                    {
                        if let Ok(mut event) = engine_webrtc_protocol::EventFrame::decode(data) {
                            room.event_seq = room.event_seq.wrapping_add(1);
                            event.seq = room.event_seq;
                            event.from = id as u16; // Server-authoritative sender stamp (Anti-spoofing)

                            let mut wire_event =
                                Vec::with_capacity(engine_webrtc_protocol::EVENT_WIRE_LEN);
                            engine_webrtc_protocol::encode_event(&event, &mut wire_event);

                            // SFU Multicast: forward guaranteed event to all other clients in the room
                            for (cid, &pid) in &room.sessions {
                                if pid != id {
                                    let _ =
                                        transport.send_to(cid, OpCode::RoomDataBytes, &wire_event);
                                }
                            }
                            if !room.spectators.is_empty() {
                                for spec in &room.spectators {
                                    let _ =
                                        transport.send_to(spec, OpCode::RoomDataBytes, &wire_event);
                                }
                            }
                        }
                        continue;
                    }

                    // QUIC-Style Unordered Movement Frame: [0x01, seq_u16, flags_u32, yaw_f32, pitch_f32, mx_f32, mz_f32] (23 bytes)
                    if data.len() == engine_webrtc_protocol::INPUT_WIRE_LEN
                        && data[0] == engine_webrtc_protocol::OP_INPUT
                    {
                        if let Ok(input) = engine_webrtc_protocol::InputFrame::decode(data) {
                            last_input_at.insert(client_id.to_string(), Instant::now());
                            if transport.is_rtc_connected(&client_id) && input.seq % 120 == 1 {
                                tracing::debug!(
                                    "[server] ℹ️ Player {id} sent a stamped input frame (seq {})",
                                    input.seq
                                );
                            }
                            // Newest-Wins auto-sync rule (QUIC datagram behavior: drop stale packets, zero HOL blocking).
                            // With rollback on, a genuinely late seq rewinds and replays instead of dropping.
                            let mut payload_20 = [0u8; 20];
                            payload_20.copy_from_slice(&data[3..23]);
                            netcode::apply_input(
                                room,
                                cfg,
                                tick_interval.as_secs_f64(),
                                &client_id,
                                id,
                                input.seq,
                                &payload_20,
                            );
                        }
                        continue;
                    }

                    last_input_at.insert(client_id.to_string(), Instant::now());

                    if room.is_ecs() {
                        // Native ECS backend: raw bytes are the WASD flags.
                        room.ecs_input(&client_id, data);
                        flush_room_outbox(room, &mut transport);
                        continue;
                    }

                    // Rule 3 (Ownership) & Rule 5 (Fault isolation):
                    // Guest allocates memory; host writes and invokes on_input safely without panicking.
                    if let Some(ref mut wasm) = room.wasm {
                        match wasm.on_alloc(data.len() as i32) {
                            Ok(ptr_i32) => {
                                let ptr = ptr_i32 as u32;
                                if let Err(e) = wasm.write_memory(ptr, data) {
                                    tracing::error!(
                                        "[server] ⚠️ failed to write memory for player {id}: {e}"
                                    );
                                } else if let Err(e) = wasm.on_input(id, ptr_i32, data.len() as i32)
                                {
                                    tracing::warn!(
                                        "[server] ⚠️ guest trap in on_input for player {id}: {e}"
                                    );
                                }
                            }
                            Err(e) => {
                                tracing::error!("[server] ⚠️ guest on_alloc failed for player {id}: {e}");
                            }
                        }
                    }
                    // Reply now instead of waiting for the next tick: typed
                    // messages from onMessage() are low-latency and independent
                    // of the continuous delta.
                    flush_room_outbox(room, &mut transport);
                }
            }
        }

        // Poll UDP socket for Unity / Unreal / Godot incoming datagrams
        if let Some(ref udp_sock) = udp_socket {
            let mut udp_buf = [0u8; 4096];
            while let Ok((amt, peer_addr)) = udp_sock.recv_from(&mut udp_buf) {
                let data = &udp_buf[..amt];

                // ---- RUDP control/data (kisi bhi join state me valid) ----
                if data.len() >= 3 && data[0] == netcode::CYG_WIRE_RELIABLE_ACK {
                    if let Some(seq) = netcode::decode_reliable_ack(data) {
                        if let Some(lane) = udp_reliable.get_mut(&peer_addr) {
                            lane.ack(seq, mono_ms());
                        }
                    }
                    continue;
                }
                if let Some((seq, payload)) = netcode::decode_reliable_msg(data) {
                    // ACK turant (client ka window khula rahe), phir payload
                    // room ko. QUIC semantics: har frame APNE seq ke saath
                    // aate hi deliver — reordering pe koi drop nahi, koi
                    // block nahi; sirf resend-duplicates suppress hote hain.
                    netcode::encode_reliable_ack(seq, &mut rudp_scratch);
                    let _ = udp_sock.send_to(&rudp_scratch, peer_addr);
                    let seen = udp_rxd_seq.entry(peer_addr).or_default();
                    if seen.insert(seq) {
                        if seen.len() > RUDP_DEDUPE_SPAN {
                            seen.retain(|s| engine_webrtc_protocol::is_newer(seq, *s) || *s == seq);
                        }
                        let server_guard = &mut server;
                        if let Ok(room) = server_guard.get_or_create(&cfg.name) {
                            room.empty_since = None;
                            if let Some(ref mut wasm) = room.wasm {
                                if let Ok(ptr) = wasm.on_alloc(payload.len() as i32) {
                                    let _ = wasm.write_memory(ptr as u32, payload);
                                    let _ = wasm.on_input(0, ptr, payload.len() as i32);
                                }
                            }
                            flush_room_outbox(room, &mut transport);
                        }
                    }
                    continue;
                }

                let server_guard = &mut server;
                let room = match server_guard.get_or_create(&cfg.name) {
                    Ok(room) => room,
                    Err(e) => {
                        tracing::error!("[server-udp] ⛔ room '{}' unavailable: {e}", cfg.name);
                        continue;
                    }
                };
                room.empty_since = None;

                let is_spec_handshake =
                    data.len() == 5 && data[0] == 0xFF && &data[1..5] == b"SPEC";

                if !data.is_empty()
                    && (data[0] == netcode::CYG_WIRE_LEAVE_ROOM
                        || data[0] == netcode::CYG_OP_LEAVE_ROOM)
                {
                    if let Some((id, sess, _)) = udp_peers.remove(&peer_addr) {
                        tracing::info!("[server-udp] {peer_addr} left room via UDP (session {sess})");
                        udp_input_rates.remove(&peer_addr);
                        udp_reliable.remove(&peer_addr);
                        udp_reliable_seq.remove(&peer_addr);
                        udp_rxd_seq.remove(&peer_addr);
                        room.sessions.remove(&sess);
                        room.schema_sent.remove(&sess);
                        if id > 0 {
                            room.remove_raw_player(id);
                        }
                    }
                    continue;
                }

                if !udp_peers.contains_key(&peer_addr) {
                    if is_spec_handshake {
                        let session_id = format!("spec_{}_{}", peer_addr.ip(), peer_addr.port());
                        udp_peers.insert(peer_addr, (0, session_id.clone(), Instant::now())); // 0 = spectator
                        tracing::info!("[server-udp] 👁️  {peer_addr} connected as SPECTATOR via UDP");
                        let _ = udp_sock.send_to(SPECTATOR_CONFIRM, peer_addr);
                    } else {
                        // Join amplification guard: max 5 joins/sec per source
                        // address. The join path is the only unauthenticated
                        // allocation (guest on_join + descriptor-sized reply).
                        if udp_join_rates.len() >= 4096 && !udp_join_rates.contains_key(&peer_addr) {
                            continue;
                        }
                        if !udp_join_rates.entry(peer_addr).or_insert_with(|| netcode::InputRateLimiter::new(5, 5.0)).allow(Instant::now()) {
                            tracing::error!("[server-udp] ⛔ join rate-limit hit by {peer_addr}");
                            continue;
                        }
                        let n = room.sessions.len() + udp_peers.len();
                        let (x, y) = (
                            60.0 + ((n as f64) * 97.0) % (cfg.width - 120.0),
                            60.0 + ((n as f64) * 61.0) % (cfg.height - 120.0),
                        );
                        let id = room.add_raw_player(x, y);
                        let session_id = format!("p{}_{}_{}", id, peer_addr.ip(), peer_addr.port());
                        udp_peers.insert(peer_addr, (id, session_id.clone(), Instant::now()));
                        udp_reliable.entry(peer_addr).or_default();
                        tracing::info!(
                            "[server-udp] {peer_addr} joined room via UDP (player id {id}, session {session_id})"
                        );

                        // 1. Send JOIN_ROOM handshake reply to the joining client so SDK marks room as joined
                        let rt = format!("{}:{}", cfg.name, session_id);
                        let mut join_reply = Vec::with_capacity(1 + 1 + rt.len() + 1 + 6 + 1);
                        join_reply.push(netcode::CYG_WIRE_JOIN_ROOM);
                        join_reply.push(rt.len() as u8);
                        join_reply.extend_from_slice(rt.as_bytes());
                        join_reply.push(6); // "schema".len()
                        join_reply.extend_from_slice(b"schema");
                        join_reply.push(0x00); // msgpack 0
                        let _ = udp_sock.send_to(&join_reply, peer_addr);
                        if !schema_descriptor.is_empty() {
                            let _ = udp_sock.send_to(&schema_descriptor, peer_addr);
                        }

                        if data.len() == 20 {
                            if let Some(ref mut wasm) = room.wasm {
                                if let Ok(ptr) = wasm.on_alloc(data.len() as i32) {
                                    let _ = wasm.write_memory(ptr as u32, data);
                                    let _ = wasm.on_input(id, ptr, data.len() as i32);
                                }
                            }
                        }
                    }
                } else if let Some((id, _peer_sess, last_seen)) = udp_peers.get_mut(&peer_addr) {
                    *last_seen = Instant::now();
                    if is_spec_handshake && *id != 0 {
                        room.remove_raw_player(*id);
                        *id = 0;
                        tracing::info!("[server-udp] 👁️  {peer_addr} upgraded to SPECTATOR via UDP");
                    } else if *id > 0 && !data.is_empty() {
                        if data[0] == netcode::CYG_WIRE_PING {
                            let _ = udp_sock.send_to(data, peer_addr);
                            continue;
                        }
                        if data.len() == 20 {
                            if let Some(ref mut wasm) = room.wasm {
                                if let Ok(ptr) = wasm.on_alloc(data.len() as i32) {
                                    let _ = wasm.write_memory(ptr as u32, data);
                                    let _ = wasm.on_input(*id, ptr, data.len() as i32);
                                }
                            }
                        }
                    }
                }
            }
        }

        // RUDP pump: due resends (Go-Back-N) — datagram loss pe automatic retry.
        if let Some(ref udp_sock) = udp_socket {
            let now = mono_ms();
            let due: Vec<SocketAddr> = udp_reliable
                .iter()
                .filter(|(_, lane)| lane.next_due().map_or(false, |d| d <= now))
                .map(|(a, _)| *a)
                .collect();
            for addr in due {
                if let Some(lane) = udp_reliable.get_mut(&addr) {
                    lane.pump(now, |frame| {
                        udp_sock.send_to(frame, addr).is_ok()
                    });
                }
            }
        }

        // Lane-classified client frames from the WebRTC data plane, drained by
        // this iteration's `poll_events`. The transport decided which channel
        // each frame came in on: an ack that arrives on `game-unreliable` is
        // `Refused` rather than `Ack`, because that lane cannot prove delivery.
        for (client_id, msg) in transport.take_inbound() {
            let room_name = transport
                .room_of(&client_id)
                .map(|r| sanitize_room_name(&r).to_string())
                .unwrap_or_else(|| cfg.name.clone());
            let Some(room) = server.rooms.get_mut(&room_name) else {
                continue;
            };
            let Some(&id) = room.sessions.get(&*client_id) else {
                continue;
            };
            match msg {
                Inbound::Input {
                    seq,
                    payload,
                    resupplied,
                } => {
                    last_input_at.insert(client_id.clone(), Instant::now());
                    if !resupplied && seq % 120 == 1 {
                        tracing::debug!(
                            "[server] ⚡ WebRTC UDP input stream active for player {id} (seq {seq})"
                        );
                    }
                    netcode::apply_input(
                        room,
                        cfg,
                        tick_interval.as_secs_f64(),
                        &client_id,
                        id,
                        seq,
                        &payload,
                    );
                }
                Inbound::Event { raw } => {
                    let Ok(mut event) = engine_webrtc_protocol::EventFrame::decode(&raw) else {
                        continue;
                    };
                    room.event_seq = room.event_seq.wrapping_add(1);
                    event.seq = room.event_seq;
                    event.from = id as u16;

                    let mut wire_event =
                        Vec::with_capacity(engine_webrtc_protocol::EVENT_WIRE_LEN);
                    engine_webrtc_protocol::encode_event(&event, &mut wire_event);

                    if event.kind == engine_webrtc_protocol::EventKind::Fire {
                        let fire_payload = 16u32.to_le_bytes();
                        if let Some(ref mut wasm) = room.wasm {
                            if let Ok(ptr) = wasm.on_alloc(4) {
                                let _ = wasm.write_memory(ptr as u32, &fire_payload);
                                let _ = wasm.on_input(id, ptr, 4);
                            }
                        }
                    }

                    transport.rtc_server().broadcast_reliable(&wire_event);

                    // Sessions that never negotiated a data channel get the
                    // event through the transport's reliable lane instead.
                    for (cid, &pid) in &room.sessions {
                        if pid != id && !transport.is_rtc_connected(cid) {
                            let _ = transport.send_to(cid, OpCode::RoomDataBytes, &wire_event);
                        }
                    }
                }
                Inbound::Ack(ack) => netcode::record_channel_ack(room, &client_id, ack),
                // SDK `sendBytes` app messages: envelope strip karke guest
                // `on_input` ko raw payload do (schema/relay rooms, vector
                // games ka input yahi hai).
                Inbound::App { data } => {
                    let type_len = data[1] as usize;
                    let Some(payload) = data.get(2 + type_len..) else {
                        continue;
                    };
                    if let Some(ref mut wasm) = room.wasm {
                        match wasm.on_alloc(payload.len() as i32) {
                            Ok(ptr_i32) => {
                                let ptr = ptr_i32 as u32;
                                if let Err(e) = wasm.write_memory(ptr, payload) {
                                    tracing::error!(
                                        "[server] ⚠️ app payload write failed for player {id}: {e}"
                                    );
                                } else if let Err(e) =
                                    wasm.on_input(id, ptr_i32, payload.len() as i32)
                                {
                                    tracing::warn!(
                                        "[server] ⚠️ guest trap in on_input (app msg) for player {id}: {e}"
                                    );
                                }
                            }
                            Err(e) => {
                                tracing::error!(
                                    "[server] ⚠️ guest on_alloc failed for player {id}: {e}"
                                );
                            }
                        }
                    }
                    flush_room_outbox(room, &mut transport);
                }
                Inbound::Other { .. } | Inbound::Refused(_) => {}
            }
        }

        // ---- fixed-tick: wasm simulates, host broadcasts the delta buffer ----
        let elapsed = last_tick.elapsed();
        if elapsed >= tick_interval {
            let mut ticks_to_run = (elapsed.as_secs_f64() / tick_interval.as_secs_f64()) as u32;
            if ticks_to_run > 3 {
                ticks_to_run = 3;
                last_tick = Instant::now();
            } else {
                last_tick += tick_interval * ticks_to_run;
            }
            let dt = tick_interval.as_secs_f64();

            tick_counter += ticks_to_run as u64;
            if tick_counter >= 300 && tick_stat_start.elapsed() >= Duration::from_secs(5) {
                let stat_dur = tick_stat_start.elapsed().as_secs_f64();
                let measured_hz = (tick_counter as f64) / stat_dur;
                tracing::debug!(
                    "[server] ⏱️ Steady Fixed Tickrate: {:.2} Hz (target: {} Hz, dt: {:.2}ms, 300 samples in {:.2}s)",
                    measured_hz,
                    cfg.tick_rate,
                    dt * 1000.0,
                    stat_dur
                );
                tick_counter = 0;
                tick_stat_start = Instant::now();
            }

            let tick_start = Instant::now();
            dead_rooms.clear();
            for (name, room) in server.rooms.iter_mut() {
                // Auto-close evaluation for hibernating rooms
                if room.is_hibernating() {
                    let is_default = name == &cfg.name;
                    if (!is_default || udp_peers.is_empty())
                        && room.reap_check(Duration::from_secs(30))
                    {
                        dead_rooms.push(name.clone());
                    }
                }
            }

            for tick_idx in 0..ticks_to_run {
                for (name, room) in server.rooms.iter_mut() {
                    // Idle Hibernation (0% CPU for empty rooms)
                    if room.is_hibernating() {
                        continue;
                    }

                    // Held seats (allowReconnection) expire on wall-clock time;
                    // only then does the guest see `onLeave`.
                    if room.expire_pending_leaves(Instant::now()) {
                        flush_room_outbox(room, &mut transport);
                    }

                    // Native ECS backend: single-threaded bevy_ecs + Rapier
                    // schedule; the delta rides the same outbox as a guest's.
                    if room.is_ecs() {
                        room.tick_ecs(dt);
                        flush_room_outbox(room, &mut transport);
                        continue;
                    }

                    // Rule 5: Fault-isolated simulation tick (a guest trap does
                    // not crash the server); the rollback checkpoint rides
                    // inside `GameRoom::tick`.
                    let packed = match room.tick(dt, cfg.rollback.enabled) {
                        Ok(p) => p,
                        Err(e) => {
                            tracing::warn!(
                                "[server] ⚠️ guest trap during on_tick in room '{name}': {e}"
                            );
                            flush_room_outbox(room, &mut transport);
                            continue;
                        }
                    };
                    let ptr = ((packed >> 32) as u32) as usize;
                    let len = (packed as u32) as usize;

                    // Typed app messages are independent of the delta buffer.
                    // Flush even when `on_tick` returned no delta, and before
                    // the delta-size guards can skip the rest of this tick.
                    flush_room_outbox(room, &mut transport);

                    // Rule 4: Bounds check outbound delta size
                    if len > engine_wasm::MAX_OUTBOUND_DELTA_SIZE {
                        tracing::warn!(
                            "[server] ⚠️ delta buffer size {len} exceeded max limit {} in room '{name}'",
                            engine_wasm::MAX_OUTBOUND_DELTA_SIZE
                        );
                        continue;
                    }

                    let is_last_tick = tick_idx + 1 == ticks_to_run;
                    if len > 0 && is_last_tick {
                        if let Some(ref mut wasm) = room.wasm {
                            if let Err(e) =
                                wasm.read_memory_into(ptr as u32, len as u32, &mut delta_buf)
                            {
                                tracing::error!(
                                    "[server] ⚠️ failed to read delta memory in room '{name}': {e}"
                                );
                                continue;
                            }
                        }
                        let delta = &delta_buf[..];

                        // ---- schema state: full row once, field patches after ----
                        if let Some(schema) = netcode::split_schema(delta) {
                            // Patch-rate gate: when a window is skipped the
                            // delta is dropped, so the next due frame must be
                            // the envelope's full snapshot to recover it.
                            if !room.should_send_patch(Instant::now()) {
                                continue;
                            }
                            let force_full = room.take_patch_skipped();
                            if force_full {
                                // Keep sending the forced keyframe until every
                                // client acknowledges it, then resume patches.
                                room.schema_sent.clear();
                            }
                            let server_tick_u32 = room.current_tick as u32;
                            room.delta_seq = room.delta_seq.wrapping_add(1);
                            let delta_seq = room.delta_seq;

                            // Schema views: decode the guest full once per tick.
                            // A viewer with a culled view gets its own filtered
                            // full; otherwise the guest full/patch is sent as-is.
                            let aoi_view = schema_aoi
                                .as_ref()
                                .and_then(|layout| layout.view(schema.full));

                            let sessions: Vec<(String, i64)> = room
                                .sessions
                                .iter()
                                .map(|(s, &id)| (s.clone(), id))
                                .collect();
                            for (session, id) in sessions {
                                let culled = match (schema_aoi.as_ref(), aoi_view.as_ref()) {
                                    (Some(layout), Some(view)) => layout.filter_culled(
                                        view,
                                        id as u32,
                                        room.current_tick,
                                        &cfg.aoi,
                                        &mut view_buf,
                                    ),
                                    _ => false,
                                };
                                // Stay on the full snapshot until the client acks
                                // (`__ox_schema_ready`); a patch that races the
                                // baseline over another transport would be lost.
                                let is_full =
                                    force_full || !room.schema_sent.contains(&session);
                                let body = if culled {
                                    &view_buf[..]
                                } else if is_full {
                                    schema.full
                                } else {
                                    schema.patch
                                };
                                let acked_seq =
                                    room.last_input_seq.get(&session).copied().unwrap_or(0);
                                netcode::wrap_delta_into(
                                    &mut delta_body,
                                    id,
                                    body,
                                    acked_seq,
                                    server_tick_u32,
                                );
                                let framed = transport.send_delta_framed(
                                    &session,
                                    delta_seq,
                                    server_tick_u32,
                                    &delta_body,
                                );
                                if framed == 0 {
                                    if is_full || culled {
                                        // Full snapshots are idempotent: the
                                        // latest one supersedes unflushed ones.
                                        let _ = transport.send_state(&session, &delta_body);
                                    } else {
                                        let _ = transport.send_to(
                                            &session,
                                            OpCode::RoomDataBytes,
                                            &delta_body,
                                        );
                                    }
                                }
                            }

                            let spectators: Vec<String> = room.spectators.iter().cloned().collect();
                            for spec in spectators {
                                let is_full = force_full || !room.schema_sent.contains(&spec);
                                let body = if is_full { schema.full } else { schema.patch };
                                netcode::wrap_delta_into(
                                    &mut delta_body,
                                    0,
                                    body,
                                    0,
                                    server_tick_u32,
                                );
                                let framed = transport.send_delta_framed(
                                    &spec,
                                    delta_seq,
                                    server_tick_u32,
                                    &delta_body,
                                );
                                if framed == 0 {
                                    if is_full {
                                        let _ = transport.send_state(&spec, &delta_body);
                                    } else {
                                        let _ = transport.send_to(
                                            &spec,
                                            OpCode::RoomDataBytes,
                                            &delta_body,
                                        );
                                    }
                                }
                            }

                        } else {
                            // Interest management: parse the delta once per tick.
                            // When the layout is recognized and AOI is on, every player
                            // gets a per-viewer culled body; otherwise everyone shares the
                            // raw multicast buffer.
                            let server_tick_u32 = room.current_tick as u32;
                            room.delta_seq = room.delta_seq.wrapping_add(1);
                            let delta_seq = room.delta_seq;
                            let aoi_sections = if cfg.aoi.enabled {
                                netcode::parse_delta_sections(delta)
                            } else {
                                None
                            };

                            // SFU Spatial Audio: Synchronize authoritative 3D positions into KD-Tree
                            if let Some((_, ref entities, _, _, _)) = aoi_sections {
                                for e in entities {
                                    room.voice_room.update_position(e.id as u32, e.pos);
                                }
                            } else if delta.len() >= 4 {
                                let count =
                                    u32::from_le_bytes(delta[0..4].try_into().unwrap_or([0; 4]))
                                        as usize;

                                for i in 0..count {
                                    let base = 4 + i * 72;
                                    if base + 32 <= delta.len() {
                                        let pid = i64::from_le_bytes(
                                            delta[base..base + 8].try_into().unwrap_or([0; 8]),
                                        );
                                        let px = f64::from_le_bytes(
                                            delta[base + 8..base + 16].try_into().unwrap_or([0; 8]),
                                        );
                                        let py = f64::from_le_bytes(
                                            delta[base + 16..base + 24]
                                                .try_into()
                                                .unwrap_or([0; 8]),
                                        );
                                        let pz = f64::from_le_bytes(
                                            delta[base + 24..base + 32]
                                                .try_into()
                                                .unwrap_or([0; 8]),
                                        );
                                        room.voice_room.update_position(pid as u32, [px, py, pz]);
                                    }
                                }
                            }

                            // 1. Broadcast to active players with their player ID
                            for (session, id) in &room.sessions {
                                if let Some((_, entities, bullet_count, bc_off, bullet_end)) =
                                    &aoi_sections
                                {
                                    let viewer_pos =
                                        entities.iter().find(|e| e.id == *id).map(|e| e.pos);
                                    netcode::filter_delta_for_viewer(
                                        entities,
                                        *bullet_count,
                                        (*bc_off + 4, *bullet_end),
                                        delta,
                                        *id,
                                        viewer_pos,
                                        room.current_tick,
                                        &cfg.aoi,
                                        &mut filtered_buf,
                                    );
                                    let acked_seq =
                                        room.last_input_seq.get(session).copied().unwrap_or(0);
                                    netcode::wrap_delta_into(
                                        &mut delta_body,
                                        *id,
                                        &filtered_buf,
                                        acked_seq,
                                        server_tick_u32,
                                    );
                                } else {
                                    let acked_seq =
                                        room.last_input_seq.get(session).copied().unwrap_or(0);
                                    netcode::wrap_delta_into(
                                        &mut delta_body,
                                        *id,
                                        delta,
                                        acked_seq,
                                        server_tick_u32,
                                    );
                                }
                                let framed = transport.send_delta_framed(
                                    session,
                                    delta_seq,
                                    server_tick_u32,
                                    &delta_body,
                                );
                                if framed == 0 {
                                    let _ = transport.send_to(
                                        session,
                                        OpCode::RoomDataBytes,
                                        &delta_body,
                                    );
                                }
                            }

                            // Flush outbound WebRTC UDP packets

                            // 2. Broadcast to spectators with ID 0 (Observer mode). They
                            // ride the same stamped channel as players, so a spectator
                            // also drops superseded frames instead of drawing them.
                            if !room.spectators.is_empty() {
                                netcode::wrap_delta_into(
                                    &mut delta_body,
                                    0,
                                    delta,
                                    0,
                                    server_tick_u32,
                                );
                                for spec_session in &room.spectators {
                                    let framed = transport.send_delta_framed(
                                        spec_session,
                                        delta_seq,
                                        server_tick_u32,
                                        &delta_body,
                                    );
                                    if framed == 0 {
                                        let _ = transport.send_to(
                                            spec_session,
                                            OpCode::RoomDataBytes,
                                            &delta_body,
                                        );
                                    }
                                }
                            }

                            // 3. Broadcast to UDP Unity / Unreal / Godot peers. One
                            // datagram, no stamp: those clients parse the delta body
                            // directly and the channel layer is WebRTC-only for now.
                            // Sent as one sendmmsg batch per tick.
                            if let Some(ref udp_sock) = udp_socket {
                                udp_batch.clear();
                                for (&peer_addr, &(id, _, _)) in &udp_peers {
                                    netcode::wrap_delta_into(
                                        &mut frame_buf,
                                        id,
                                        delta,
                                        0,
                                        server_tick_u32,
                                    );
                                    udp_batch.push((peer_addr, frame_buf.clone()));
                                }
                                engine_net::transport::send_datagram_batch(udp_sock, &udp_batch);
                            }
                        } // legacy delta layout
                    }
                }
                // One sample per batch, as the pinned sim plane does: this is the
                // wall cost of keeping the room's ticks and broadcast current.
                recorder.note_tick(tick_start.elapsed());
                window_ticks += ticks_to_run as u64;
            }

            // Cleanup inactive UDP peers
            let now = Instant::now();
            timed_out_udp.clear();
            udp_peers.retain(|&addr, (id, sess, last_seen)| {
                if now.duration_since(*last_seen) > Duration::from_millis(3000) {
                    timed_out_udp.push((addr, *id, sess.clone()));
                    false
                } else {
                    true
                }
            });
            for (addr, id, sess) in timed_out_udp.drain(..) {
                tracing::info!("[server-udp] disconnected (timeout): {addr}");
                udp_input_rates.remove(&addr);
                if let Some(room) = server.rooms.get_mut(&cfg.name) {
                    room.sessions.remove(&sess);
                    room.schema_sent.remove(&sess);
                    if id > 0 {
                        room.remove_raw_player(id);
                    }
                }
            }

            for name in dead_rooms.drain(..) {
                if let Some(mut room) = server.rooms.remove(&name) {
                    if let Some(snapshot) = room.export_snapshot() {
                        server.save_snapshot(&name, snapshot);
                    }
                    // Last guest callback: flush scores / release resources.
                    room.dispose();
                    tracing::info!("[server] room '{name}' disposed and state saved (empty 30s)");
                }
            }
            transport.flush().expect("flush");
        }

        // ---- telemetry: one second of ticks plus the sessions this loop owns
        if publish_start.elapsed() >= Duration::from_secs(1) {
            let window_sec = publish_start.elapsed().as_secs_f64().max(1e-9);
            let mut sessions: Vec<SessionTelemetry> = Vec::new();
            let mut players = 0u32;
            let mut webrtc_live = 0u32;
            for room in server.rooms.values() {
                players += room.sessions.len() as u32;
                let members = room.sessions.iter().map(|(s, &id)| (s, Some(id)));
                let watching = room.spectators.iter().map(|s| (s, None));
                for (session, player_id) in members.chain(watching) {
                    // One session, one transport: the browser that negotiated a
                    // data channel is counted on WebRTC, where its packets
                    // actually arrive.
                    let live = transport.is_rtc_connected(session);
                    webrtc_live += live as u32;
                    sessions.push(SessionTelemetry {
                        session: session.clone(),
                        transport: if live { "webrtc" } else { "websocket" }.to_string(),
                        player_id,
                        acked_seq: room.last_input_seq.get(session).map(|&seq| seq as u32),
                        idle_ms: last_input_at
                            .get(session)
                            .map(|t| t.elapsed().as_millis() as u64),
                    });
                }
            }
            for (&addr, &(id, ref _sess, last_seen)) in &udp_peers {
                sessions.push(SessionTelemetry {
                    session: addr.to_string(),
                    transport: "native-udp".to_string(),
                    player_id: (id > 0).then_some(id),
                    // The native path hands the guest raw bytes and keeps no
                    // per-address seq watermark, so it has nothing to echo.
                    acked_seq: None,
                    idle_ms: Some(last_seen.elapsed().as_millis() as u64),
                });
            }
            // Reset only when a window really rolled: if the recorder declined
            // this call, the next one covers both seconds and its Hz holds.
            if recorder.maybe_publish(
                &bus,
                PublishReport {
                    tick_hz_est: window_ticks as f64 / window_sec,
                    tick_rate_hz: cfg.tick_rate,
                    players,
                    ws_sessions: (transport.ccu() as u32).saturating_sub(webrtc_live),
                    webrtc_sessions: webrtc_live,
                    udp_sessions: udp_peers.len() as u32,
                    sessions,
                    session_cap: cfg.max_clients.max(0) as usize,
                },
            ) {
                publish_start = Instant::now();
                window_ticks = 0;
            }
        }

        // ---- DEV HOT-RELOAD CHECK: watch room.ts (and room_logic.wasm) ----
        if last_wasm_check.elapsed() >= Duration::from_millis(500) {
            last_wasm_check = Instant::now();
            let mut recompilation_triggered = false;

            // 0. Control-plane reloads: the console's wasm upload/activate
            // endpoints (`/v2/console/games/{gameId}/wasm*`, engine-api
            // `WasmService`) publish a `WasmReload` on this bus whenever the
            // active `room_modules` row changes. Drain them and force the same
            // Level 1 in-memory swap the file watcher drives below.
            let mut console_reload = false;
            while let Ok(reload) = bus.reloads_rx.try_recv() {
                console_reload = true;
                tracing::info!(
                    "[server] 📦 console wasm reload: game '{}' {} v{} (sha256 {}…)",
                    reload.game_key,
                    match reload.kind {
                        engine_bus::ReloadKind::Upload => "uploaded",
                        engine_bus::ReloadKind::Rollback => "rolled back to",
                    },
                    reload.version,
                    &reload.sha256[..reload.sha256.len().min(12)]
                );
            }
            if console_reload && !wasm_available {
                tracing::warn!(
                    "[server] ⚠️ console reload ignored: this plane booted without a \
                     room module at {} — nothing to hot-swap onto yet",
                    wasm_path.display()
                );
            }

            // 1. Detect changes in the TypeScript room source (room.ts)
            if let Ok(src_current) = std::fs::metadata(&source_path).and_then(|m| m.modified()) {
                if loaded_src_time != Some(src_current) {
                    tracing::info!(
                        "[server] 📝 Detected change in {} — recompiling room logic...",
                        cfg.source.display()
                    );
                    match crate::compile::build(project_dir, cfg) {
                        Ok(_) => {
                            loaded_src_time = Some(src_current);
                            recompilation_triggered = true;
                        }
                        Err(e) => {
                            tracing::error!(
                                "[server] ⚠️ Compilation error in {}: {e}",
                                cfg.source.display()
                            );
                            // Record mtime so we don't spam compile errors every 500ms
                            loaded_src_time = Some(src_current);
                        }
                    }
                }
            }

            // 2. Detect changes in compiled wasm (either from step 1 or external compiler).
            // Skipped when the process booted without a room module — there is
            // nothing to hot-swap onto. A step-0 console reload forces the same
            // path even when the file mtime is unchanged.
            if wasm_available {
                if let Ok(current) = std::fs::metadata(wasm_path).and_then(|m| m.modified()) {
                    if console_reload
                        || loaded_wasm_time != Some(current)
                        || recompilation_triggered
                    {
                        match runtime
                            .load_or_compile_cached(wasm_path)
                            .and_then(|module| {
                                let linker = rooms::build_host_linker(&runtime, server.permissions);
                                runtime.prepare(&linker, &module)
                            }) {
                            Ok(pre) => {
                                loaded_wasm_time = Some(current);
                                tracing::info!("[server] 🔥 DEV HOT-RELOAD: room logic updated!");

                                // 🔥 Hot-swap all actively running rooms without
                                // disconnecting players (`GameRoom::reload_wasm`
                                // re-joins every active player per room).
                                server.hot_reload(pre, cfg.width, cfg.height);
                            }
                            Err(e) => {
                                tracing::error!("[server] ⚠️ hot-reload compilation failed: {e}");
                            }
                        }
                    }
                }
            }
        }

        transport.flush().expect("flush");
    }
}

#[cfg(test)]
mod loop_transport_tests {
    use super::*;

    fn transport() -> LoopTransport {
        LoopTransport::new(WebRtcTransport::new("127.0.0.1:0".parse().unwrap()).unwrap())
    }

    /// A rejected join for a session the transport never accepted must be a
    /// no-op, not a panic or a phantom disconnect. This is the same surface
    /// `reject_room` uses on the live path (send ERROR, close WITH_ERROR,
    /// flush) now that it calls the transport directly instead of a channel.
    #[test]
    fn reject_room_on_an_unknown_session_is_a_noop() {
        let mut t = transport();
        reject_room(&mut t, "sess_missing", "room is full");

        let mut events = Vec::new();
        t.poll_events(Duration::ZERO, &mut events).unwrap();
        assert!(events.is_empty(), "unexpected events: {events:?}");
        assert_eq!(t.ccu(), 0);
    }

    #[test]
    fn local_addr_reports_the_bound_socket() {
        let t = transport();
        assert_ne!(t.local_addr().unwrap().port(), 0);
    }

    #[test]
    fn room_name_sanitizer_drops_query_and_session_suffix() {
        assert_eq!(sanitize_room_name("smoke"), "smoke");
        assert_eq!(sanitize_room_name("smoke?sessionId=alice&token=x"), "smoke");
        assert_eq!(sanitize_room_name("smoke?room=ignored"), "smoke");
        assert_eq!(sanitize_room_name("smoke/sessionId=alice"), "smoke/");
    }
}
