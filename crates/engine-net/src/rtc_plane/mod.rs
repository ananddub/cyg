//! # Tokio-resident WebRTC data plane
//!
//! This is the WebRTC data plane that runs **inside Tokio**, entirely separate
//! from the `mio` simulation thread. It owns:
//!
//! - a single bound UDP socket,
//! - one Tokio runtime driven by **one dedicated OS thread**,
//! - a demultiplexer task that routes datagrams by remote [`SocketAddr`] to
//!   per-session tasks through bounded channels,
//! - one task per browser client, each owning its own [`RtcSession`] (`str0m::Rtc`).
//!
//! **No session shares a mutex with another.** Per-session state lives only in
//! its own task; the demux task owns the address registry (no lock — it is the
//! sole reader of the socket), and the only shared structures are *registries of
//! senders* (`outbox`, `player_index`) that the hot downlink path reads to find a
//! session's inbox. Nothing here touches crypto or sockets on the mio thread:
//! DTLS/SCTP/scheduling all happen on the plane thread.
//!
//! ## Bridge contract (for the root package)
//!
//! The signaling layer (Axum/WebSocket, owned by another agent) drives the plane
//! through [`PlaneHandle`]: `create_session` → SDP offer, then `apply_answer` /
//! `add_candidate` for the browser's SDP and trickle ICE. The game loop pushes
//! world deltas with [`PlaneHandle::send_delta`] and drains uplink client input
//! with [`PlaneHandle::recv_uplink`].
//!
//! The root package bridges these plane types onto its crossbeam bus. The exact
//! type names to adopt are: [`RtcInput`] (client→server, emitted by the plane),
//! [`RtcDownlink`] (server→client, consumed by the plane), and [`UplinkEvent`]
//! (the plane's uplink stream). They are deliberately plain data — no str0m or
//! Tokio types leak across them — so the bridge can move them over `crossbeam`.

mod session;

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

use tokio::net::UdpSocket;
use tokio::sync::{mpsc, oneshot};

pub use session::{
    LABEL_RELIABLE, LABEL_UNRELIABLE, Outbound, PlaneChannel, RtcSession, SessionEvent,
};

/// Inbound (uplink) one client input frame emitted by the plane.
///
/// `body` is the **raw** DataChannel payload; the plane intentionally does not
/// decode the game-specific input header. `seq`/`tick_hint` are best-effort
/// placeholders (zero) that the root bridge overwrites when it decodes `body`
/// onto its crossbeam bus. They exist here so the bridge has one struct to carry
/// plane→sim without a second allocation.
#[derive(Debug, Clone)]
pub struct RtcInput {
    pub player_id: Arc<str>,
    pub seq: u16,
    pub tick_hint: u32,
    pub body: Vec<u8>,
}

/// Outbound (downlink) one server delta handed to the plane.
///
/// `body` is an un-fragmented delta payload; the plane fragments it with the
/// codec (`MAX_DELTA_CHUNK` per frame) and writes the frames to the unreliable
/// channel. A `body` larger than `MAX_DELTA_BODY` is rejected, never truncated.
/// `seq` is room-wide (same for every recipient of a tick) and `tick` mirrors
/// `room.current_tick`.
#[derive(Debug, Clone)]
pub struct RtcDownlink {
    pub player_id: Arc<str>,
    pub tick: u32,
    pub seq: u16,
    pub body: Vec<u8>,
}

/// A message leaving the plane toward the game loop / bridge.
#[derive(Debug, Clone)]
pub enum UplinkEvent {
    /// Client connected and both channels are established.
    Connected { player_id: Arc<str> },
    /// Client disconnected or the session was disposed.
    Disconnected { player_id: Arc<str> },
    /// Raw unreliable-channel input from a client.
    Input(RtcInput),
    /// Reliable-channel payload from a client (events / acks).
    Reliable { player_id: Arc<str>, body: Vec<u8> },
}

/// Errors returned across the plane's public surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RtcPlaneError {
    Signaling(String),
    Transport(String),
    ChannelNotOpen(String),
    DeltaTooLarge(usize),
    NoSuchSession,
    PlayerUnknown,
    PlaneGone,
}

impl std::fmt::Display for RtcPlaneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RtcPlaneError::Signaling(s) => write!(f, "signaling error: {s}"),
            RtcPlaneError::Transport(s) => write!(f, "transport error: {s}"),
            RtcPlaneError::ChannelNotOpen(label) => write!(f, "channel '{label}' not open"),
            RtcPlaneError::DeltaTooLarge(n) => write!(f, "delta body of {n} bytes is oversized"),
            RtcPlaneError::NoSuchSession => write!(f, "no such session"),
            RtcPlaneError::PlayerUnknown => write!(f, "unknown player id"),
            RtcPlaneError::PlaneGone => write!(f, "plane is shut down"),
        }
    }
}

impl std::error::Error for RtcPlaneError {}

/// Bounded mailbox every datagram/producer hands to a single session task.
/// `Datagram` and `Delta` share an `Arc<[u8]>` so a broadcast does not clone the
/// payload once per session.
enum SessionInbound {
    Datagram { from: SocketAddr, bytes: Arc<[u8]> },
    Delta { seq: u16, tick: u32, body: Arc<[u8]> },
    Reliable { body: Arc<[u8]> },
    Answer { sdp: String },
    Candidate { sdp: String },
    Close,
}

/// Cold-path control from [`PlaneHandle`] into the demux task.
enum PlaneCommand {
    CreateSession {
        session_id: Arc<str>,
        player_id: Arc<str>,
        reply: oneshot::Sender<Result<String, RtcPlaneError>>,
    },
    /// `apply_answer` / `add_candidate` / `close_session` resolve the session id
    /// to its inbox inside the demux (the sole owner of the live session map).
    Forward { session_id: Arc<str>, msg: SessionInbound },
    Shutdown,
}

/// Session→demux notices (remote-address binding / teardown).
enum SessionNotice {
    RemoteAddrBound { session_id: Arc<str>, addr: SocketAddr },
    Gone { session_id: Arc<str> },
}

/// Tunables for [`Plane::spawn`].
#[derive(Debug, Clone, Copy)]
pub struct PlaneConfig {
    /// Per-session bounded inbox depth. A full inbox means the session task is
    /// wedged; downlink drops are expected (never block the producer).
    pub session_inbox_capacity: usize,
}

impl Default for PlaneConfig {
    fn default() -> Self {
        Self { session_inbox_capacity: 512 }
    }
}

/// Registries the hot downlink path reads without going through the demux. These
/// hold *senders*, never session state — locking them does not serialize two
/// sessions against each other.
#[derive(Clone)]
struct Registries {
    /// session id -> its inbox sender.
    outbox: Arc<Mutex<HashMap<Arc<str>, mpsc::Sender<SessionInbound>>>>,
    /// player id -> session id.
    player_index: Arc<Mutex<HashMap<Arc<str>, Arc<str>>>>,
}

impl Registries {
    fn lock_outbox(&self) -> std::sync::MutexGuard<'_, HashMap<Arc<str>, mpsc::Sender<SessionInbound>>> {
        self.outbox.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn lock_players(&self) -> std::sync::MutexGuard<'_, HashMap<Arc<str>, Arc<str>>> {
        self.player_index.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Handle to the running plane, cloneable and `Send + Sync` for use from the
/// signaling thread and the game loop thread.
#[derive(Clone)]
pub struct PlaneHandle {
    cmd_tx: mpsc::UnboundedSender<PlaneCommand>,
    registries: Registries,
    uplink: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<UplinkEvent>>>,
}

impl PlaneHandle {
    /// Create a session for `session_id` bound to `player_id`; returns the SDP
    /// offer to hand to the browser.
    pub async fn create_session(
        &self,
        session_id: Arc<str>,
        player_id: Arc<str>,
    ) -> Result<String, RtcPlaneError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(PlaneCommand::CreateSession { session_id, player_id, reply: reply_tx })
            .map_err(|_| RtcPlaneError::PlaneGone)?;
        reply_rx.await.map_err(|_| RtcPlaneError::PlaneGone)?
    }

    /// Deliver the browser's SDP answer for a session we offered.
    pub async fn apply_answer(&self, session_id: Arc<str>, sdp: String) -> Result<(), RtcPlaneError> {
        self.forward(session_id, SessionInbound::Answer { sdp }).await
    }

    /// Deliver a trickle ICE candidate from the browser.
    pub async fn add_candidate(&self, session_id: Arc<str>, candidate: String) -> Result<(), RtcPlaneError> {
        self.forward(session_id, SessionInbound::Candidate { sdp: candidate }).await
    }

    /// Close and dispose a session.
    pub async fn close_session(&self, session_id: Arc<str>) -> Result<(), RtcPlaneError> {
        self.forward(session_id, SessionInbound::Close).await
    }

    async fn forward(&self, session_id: Arc<str>, msg: SessionInbound) -> Result<(), RtcPlaneError> {
        self.cmd_tx
            .send(PlaneCommand::Forward { session_id, msg })
            .map_err(|_| RtcPlaneError::PlaneGone)
    }

    /// Push a delta to a player. Non-blocking: if the session's bounded inbox is
    /// full the delta is dropped (a stale tick is superseded within ~20 ms anyway),
    /// so the game-loop producer is never stalled by a slow client. Returns whether
    /// the delta was accepted into the session inbox.
    pub fn send_delta(&self, downlink: &RtcDownlink) -> bool {
        self.route_to_session(&downlink.player_id, |tx| {
            tx.try_send(SessionInbound::Delta {
                seq: downlink.seq,
                tick: downlink.tick,
                body: Arc::from(downlink.body.as_slice()),
            })
            .is_ok()
        })
    }

    /// Push a payload to a player's reliable channel. Non-blocking like
    /// [`Self::send_delta`].
    pub fn send_reliable(&self, player_id: &Arc<str>, body: &[u8]) -> bool {
        let body: Arc<[u8]> = Arc::from(body);
        self.route_to_session(player_id, move |tx| {
            tx.try_send(SessionInbound::Reliable { body: body.clone() }).is_ok()
        })
    }

    /// Shared downlink routing: resolve player → session → inbox sender and apply
    /// a non-blocking `try_send`. Never blocks the producer; `false` if the player
    /// is unknown, the session is gone, or the inbox is full.
    fn route_to_session(
        &self,
        player_id: &Arc<str>,
        send: impl FnOnce(&mpsc::Sender<SessionInbound>) -> bool,
    ) -> bool {
        let session_id = {
            let index = self.registries.lock_players();
            match index.get(player_id) {
                Some(sid) => sid.clone(),
                None => return false,
            }
        };
        let outbox = self.registries.lock_outbox();
        match outbox.get(&session_id) {
            Some(tx) => send(tx),
            None => false,
        }
    }

    /// The "which session belongs to player id" lookup.
    pub fn session_for_player(&self, player_id: &str) -> Option<Arc<str>> {
        self.registries.lock_players().get(player_id).cloned()
    }

    /// Number of live sessions with a registered inbox.
    pub fn session_count(&self) -> usize {
        self.registries.lock_outbox().len()
    }

    /// Await the next uplink event. The bridge drains this onto its crossbeam bus.
    pub async fn recv_uplink(&self) -> Option<UplinkEvent> {
        self.uplink.lock().await.recv().await
    }
}

/// The WebRTC data plane. Spawns its own Tokio runtime on one OS thread and a
/// single bound UDP socket.
pub struct Plane {
    thread: Option<JoinHandle<()>>,
    cmd_tx: mpsc::UnboundedSender<PlaneCommand>,
}

impl Plane {
    /// Bind the UDP socket on the *calling* thread (so the bound address is known
    /// synchronously), then hand it to a dedicated plane thread that drives the
    /// Tokio runtime, the demux task and every session task.
    ///
    /// `announced_addr` is advertised as the server host ICE candidate and stamped
    /// as the destination on inbound datagrams; it must be the concrete address
    /// clients actually send to (not `0.0.0.0` unless the demux resolves it).
    pub fn spawn(
        bind_addr: SocketAddr,
        announced_addr: SocketAddr,
        config: PlaneConfig,
    ) -> std::io::Result<(Plane, PlaneHandle)> {
        let std_socket = std::net::UdpSocket::bind(bind_addr)?;
        std_socket.set_nonblocking(true)?;
        let local_addr = std_socket.local_addr()?;
        let local_dest = if announced_addr.ip().is_unspecified() {
            SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), local_addr.port())
        } else {
            announced_addr
        };

        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (uplink_tx, uplink_rx) = mpsc::unbounded_channel();
        let (notice_tx, notice_rx) = mpsc::unbounded_channel();
        let registries = Registries {
            outbox: Arc::new(Mutex::new(HashMap::new())),
            player_index: Arc::new(Mutex::new(HashMap::new())),
        };

        let handle = PlaneHandle {
            cmd_tx: cmd_tx.clone(),
            registries: registries.clone(),
            uplink: Arc::new(tokio::sync::Mutex::new(uplink_rx)),
        };

        let thread = std::thread::Builder::new()
            .name("rtc-plane".to_string())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_time()
                    .enable_io()
                    .build()
                    .expect("tokio current-thread runtime");
                rt.block_on(async move {
                    let socket = Arc::new(UdpSocket::from_std(std_socket).expect("udp from_std"));
                    demux_loop(socket, local_dest, cmd_rx, notice_rx, notice_tx, uplink_tx, registries, config)
                        .await;
                });
            })?;

        Ok((Plane { thread: Some(thread), cmd_tx }, handle))
    }

    /// Signal the plane to stop and join its thread.
    pub fn shutdown(mut self) {
        let _ = self.cmd_tx.send(PlaneCommand::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Plane {
    fn drop(&mut self) {
        // Best-effort stop if the owner forgot to call `shutdown()`.
        if let Some(thread) = self.thread.take() {
            let _ = self.cmd_tx.send(PlaneCommand::Shutdown);
            let _ = thread.join();
        }
    }
}

/// The demultiplexer task: sole reader of the UDP socket, owner of the address
/// registry, spawner of session tasks. Runs on the single plane thread, so its
/// maps need no lock (only the shared sender registries are locked, and briefly).
async fn demux_loop(
    socket: Arc<UdpSocket>,
    local_dest: SocketAddr,
    mut cmd_rx: mpsc::UnboundedReceiver<PlaneCommand>,
    mut notice_rx: mpsc::UnboundedReceiver<SessionNotice>,
    notice_tx: mpsc::UnboundedSender<SessionNotice>,
    uplink_tx: mpsc::UnboundedSender<UplinkEvent>,
    registries: Registries,
    config: PlaneConfig,
) {
    let mut sessions: HashMap<Arc<str>, mpsc::Sender<SessionInbound>> = HashMap::new();
    let mut addr_routes: HashMap<SocketAddr, Arc<str>> = HashMap::new();
    // Sessions that have not yet pinned a remote address receive a broadcast copy
    // of every unknown-address datagram and decide via `accepts`.
    let mut unbound: HashSet<Arc<str>> = HashSet::new();

    let mut buf = vec![0u8; 65_535];

    loop {
        tokio::select! {
            recv = socket.recv_from(&mut buf), if true => {
                match recv {
                    Ok((n, from)) => {
                        if n > 0 {
                            let bytes: Arc<[u8]> = Arc::from(&buf[..n]);
                            route_datagram(from, bytes, &addr_routes, &unbound, &sessions);
                        }
                    }
                    // A closed socket surfaces here; the loop continues to serve
                    // commands/notices until Shutdown so sessions tear down cleanly.
                    Err(_) => {}
                }
            }
            cmd = cmd_rx.recv() => {
                let Some(cmd) = cmd else { break };
                match cmd {
                    PlaneCommand::Shutdown => break,
                    PlaneCommand::CreateSession { session_id, player_id, reply } => {
                        let outcome = create_session(
                            session_id,
                            player_id,
                            local_dest,
                            &socket,
                            &mut sessions,
                            &mut unbound,
                            &registries,
                            &notice_tx,
                            &uplink_tx,
                            config.session_inbox_capacity,
                        );
                        match outcome {
                            Ok(offer) => { let _ = reply.send(Ok(offer)); }
                            Err(err) => { let _ = reply.send(Err(err)); }
                        }
                    }
                    PlaneCommand::Forward { session_id, msg } => {
                        let closing = matches!(msg, SessionInbound::Close);
                        if let Some(tx) = sessions.get(&session_id) {
                            // Control messages go through the bounded inbox; if it is
                            // full the session is wedged, so drop (Close still tears
                            // down below when the task observes the drop of its inbox).
                            let _ = tx.try_send(msg);
                        } else if closing {
                            unbind_player(&session_id, &registries);
                        }
                    }
                }
            }
            notice = notice_rx.recv() => {
                match notice {
                    Some(SessionNotice::RemoteAddrBound { session_id, addr }) => {
                        unbound.remove(&session_id);
                        addr_routes.insert(addr, session_id);
                    }
                    Some(SessionNotice::Gone { session_id }) => {
                        sessions.remove(&session_id);
                        unbound.remove(&session_id);
                        addr_routes.retain(|_, sid| sid != &session_id);
                        unbind_player(&session_id, &registries);
                    }
                    // notice_rx closed only if every session task dropped; keep serving.
                    None => {}
                }
            }
        }
    }

    // On shutdown, ask every live session to stop.
    for (_sid, tx) in sessions.iter() {
        let _ = tx.send(SessionInbound::Close);
    }
}

/// Route one received datagram. Fast path is a known remote address; otherwise
/// copy the (Arc-backed) datagram to every session still searching for its peer.
fn route_datagram(
    from: SocketAddr,
    bytes: Arc<[u8]>,
    addr_routes: &HashMap<SocketAddr, Arc<str>>,
    unbound: &HashSet<Arc<str>>,
    sessions: &HashMap<Arc<str>, mpsc::Sender<SessionInbound>>,
) {
    if let Some(sid) = addr_routes.get(&from) {
        if let Some(tx) = sessions.get(sid) {
            let _ = tx.try_send(SessionInbound::Datagram { from, bytes });
        }
        return;
    }
    for sid in unbound {
        if let Some(tx) = sessions.get(sid) {
            let _ = tx.try_send(SessionInbound::Datagram { from, bytes: bytes.clone() });
        }
    }
}

/// Build a session and spawn its task. Runs synchronously inside the demux task
/// (no await), so it mutates the demux-owned maps directly.
#[allow(clippy::too_many_arguments)]
fn create_session(
    session_id: Arc<str>,
    player_id: Arc<str>,
    local_dest: SocketAddr,
    socket: &Arc<UdpSocket>,
    sessions: &mut HashMap<Arc<str>, mpsc::Sender<SessionInbound>>,
    unbound: &mut HashSet<Arc<str>>,
    registries: &Registries,
    notice_tx: &mpsc::UnboundedSender<SessionNotice>,
    uplink_tx: &mpsc::UnboundedSender<UplinkEvent>,
    inbox_capacity: usize,
) -> Result<String, RtcPlaneError> {
    let (rtc, offer_sdp) = RtcSession::new_server(session_id.clone(), local_dest, &[local_dest])?;

    let (tx, rx) = mpsc::channel::<SessionInbound>(inbox_capacity.max(1));
    sessions.insert(session_id.clone(), tx.clone());
    unbound.insert(session_id.clone());
    registries.lock_outbox().insert(session_id.clone(), tx);
    registries.lock_players().insert(player_id.clone(), session_id.clone());

    tokio::spawn(session_task(
        rtc,
        player_id,
        rx,
        Arc::clone(socket),
        uplink_tx.clone(),
        notice_tx.clone(),
    ));

    Ok(offer_sdp)
}

/// Remove a session's player binding once its task is gone.
fn unbind_player(session_id: &Arc<str>, registries: &Registries) {
    let mut players = registries.lock_players();
    players.retain(|_, sid| sid != session_id);
    registries.lock_outbox().remove(session_id);
}

/// One browser peer, driven as its own task on the plane runtime. The loop is the
/// canonical str0m async pump: wait for the sooner of an inbound message or the
/// engine's next timeout, feed exactly one input, then flush all output.
async fn session_task(
    mut rtc: RtcSession,
    player_id: Arc<str>,
    mut inbox: mpsc::Receiver<SessionInbound>,
    socket: Arc<UdpSocket>,
    uplink_tx: mpsc::UnboundedSender<UplinkEvent>,
    notice_tx: mpsc::UnboundedSender<SessionNotice>,
) {
    loop {
        // Wait for the sooner of: an inbound message, or poll_timeout firing.
        let delay = rtc.timeout_delay();
        let input = tokio::select! {
            biased;
            maybe = inbox.recv() => maybe,
            _ = tokio::time::sleep(delay) => {
                let out = rtc.feed_timeout(Instant::now());
                if pump_flush(&mut rtc, out, &socket, &uplink_tx, &notice_tx, &player_id).await {
                    break;
                }
                continue;
            }
        };

        let Some(inbound) = input else { break };
        let out = match inbound {
            SessionInbound::Datagram { from, bytes } => {
                let now = Instant::now();
                let bound = rtc.remote_addr() == Some(from);
                if bound || rtc.accepts(now, from, &bytes) {
                    rtc.feed_datagram(now, from, &bytes).unwrap_or_default()
                } else {
                    Vec::new()
                }
            }
            SessionInbound::Delta { seq, tick, body } => {
                // Channel may not be open yet or the body is oversized: drop the
                // delta (stale), never error the loop.
                rtc.send_delta(seq, tick, &body).unwrap_or_default()
            }
            SessionInbound::Reliable { body } => rtc.send_reliable(&body).unwrap_or_default(),
            SessionInbound::Answer { sdp } => rtc.accept_answer(&sdp).unwrap_or_default(),
            SessionInbound::Candidate { sdp } => rtc.add_remote_candidate(&sdp).unwrap_or_default(),
            SessionInbound::Close => break,
        };

        if pump_flush(&mut rtc, out, &socket, &uplink_tx, &notice_tx, &player_id).await {
            break;
        }
    }

    let _ = notice_tx.send(SessionNotice::Gone { session_id: rtc.session_id().to_string().into() });
    let _ = uplink_tx.send(UplinkEvent::Disconnected { player_id });
}

/// Deliver the output of one engine drain: datagrams to the socket, channel data
/// to the uplink, events to the demux. Returns `true` if the session is closed and
/// the task should stop.
async fn pump_flush(
    rtc: &mut RtcSession,
    outbound: Vec<Outbound>,
    socket: &UdpSocket,
    uplink_tx: &mpsc::UnboundedSender<UplinkEvent>,
    notice_tx: &mpsc::UnboundedSender<SessionNotice>,
    player_id: &Arc<str>,
) -> bool {
    let session_id = rtc.session_id().clone();
    for out in outbound {
        match out {
            Outbound::Datagram { destination, contents } => {
                // Fire-and-forget: a UDP send that fails is an ICE glitch, not fatal.
                let _ = socket.send_to(&contents, destination).await;
            }
            Outbound::Data { channel, payload, .. } => match rtc.channel_role(channel) {
                Some(PlaneChannel::Unreliable) => {
                    let _ = uplink_tx.send(UplinkEvent::Input(RtcInput {
                        player_id: player_id.clone(),
                        seq: 0,
                        tick_hint: 0,
                        body: payload,
                    }));
                }
                Some(PlaneChannel::Reliable) | None => {
                    let _ = uplink_tx.send(UplinkEvent::Reliable {
                        player_id: player_id.clone(),
                        body: payload,
                    });
                }
            },
            Outbound::Event(event) => match event {
                SessionEvent::Connected => {
                    let _ = uplink_tx.send(UplinkEvent::Connected { player_id: player_id.clone() });
                }
                SessionEvent::RemoteAddrBound { addr } => {
                    let _ = notice_tx.send(SessionNotice::RemoteAddrBound { session_id: session_id.clone(), addr });
                }
                SessionEvent::Closed => return true,
                SessionEvent::ChannelOpen { .. } | SessionEvent::ChannelClose { .. } => {}
            },
        }
    }
    false
}
