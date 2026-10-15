//! WS signaling: the browser ↔ server channel that sets up a data plane.
//!
//! One endpoint, `GET /ws/signal?seat=<seatToken>`. The seat token minted by
//! [`crate::matchmake`] **is** the authentication: it is an HMAC over
//! `"<room>:<expiry>"`, so this handler learns the room and the seat from a
//! value the client cannot forge, and a caller that never did `matchmake`
//! cannot open a signaling session at all.
//!
//! What it does and does not do:
//!
//! * It relays. SDP offers and ICE candidates go to the data plane
//!   ([`SignalHub::take_signals`]) and answers come back, through the same
//!   per-seat route this plane uses to deliver the operator's ICE list
//!   ([`SignalHub::publish`]). No DTLS, no crypto, no peer state lives here.
//! * It never trusts a `seat` field inside a message body. The seat is fixed at
//!   upgrade time, from the verified token.
//! * It never blocks on the data plane. Both directions are bounded queues; a
//!   full queue drops and counts, it does not stall a browser's socket task.
//!
//! Message shapes (both directions, one JSON object per WS text frame):
//!
//! ```text
//! C→S  {"type":"offer","sdp":"v=0\r\n…"}
//!      {"type":"candidate","candidate":"candidate:1 1 udp 123 192.168.1.5 49300 typ host","sdpMid":"0","sdpMLineIndex":0}
//!      {"type":"close"}
//!
//! S→C  {"type":"ice","config":[{"urls":["stun:…"]}]}
//!      {"type":"answer","sdp":"v=0\r\n…"}
//!      {"type":"candidate","candidate":"…","sdpMid":"0","sdpMLineIndex":0}
//!      {"type":"error","message":"…"}
//! ```
//!
//! The candidate m-line index is `sdpMLineIndex` on the wire — the spelling
//! `RTCIceCandidateInit` itself uses, and the one `sdk/web/openoxide.js` emits
//! and reads (`sdpMLineIndex: e.candidate.sdpMLineIndex`). `sdpIndex` is
//! accepted inbound as an alias, so either spelling works from a browser.
//!
//! Lifecycle: every socket that was accepted costs the data plane exactly one
//! `Close` — from a client `{"type":"close"}`, a WS Close frame, a socket
//! error, or a cancelled task — posted by [`CloseGuard::drop`] and by nothing
//! else. A reconnecting seat replaces its route, and the superseded socket's
//! teardown cannot delete the replacement: each route records the attach
//! generation that owns it and a stale [`SeatReplies`] detaches nothing.
//!
//! Queues here are `tokio::sync::mpsc`, not the crossbeam channels in
//! [`engine_bus`]: every consumer of signaling is an async task on this plane,
//! and a `recv().await` must yield the worker thread rather than park it.
//!
//! A browser is never left in silence: [`SignalHub::post`] refuses a message
//! while no data plane has claimed [`SignalHub::take_signals`], and the socket
//! answers `{"type":"error"}` instead of queueing an offer nobody will read.
//! The refusal is a read of the hand-over slot, not a claim of it, so a data
//! plane that starts later still gets the queue.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::Router;
use axum::extract::{Query, State};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering::Relaxed;
use tokio::sync::mpsc::{Receiver, Sender, error::TrySendError};

use crate::ControlState;
use crate::seats::Seat;

/// Replies buffered per socket before that client's signaling is wedged.
/// Signaling is low-rate: 64 is several multiples of a full ICE candidate sweep.
pub const PER_SEAT_QUEUE: usize = 64;

/// Shared capacity for everything heading to the data plane.
pub const INBOUND_QUEUE: usize = 4096;

/// Signaling from a browser, after this handler stamped it with a seat it
/// verified itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignalIn {
    Offer { seat: Seat, sdp: String },
    Candidate {
        seat: Seat,
        candidate: String,
        sdp_mid: Option<String>,
        sdp_index: Option<u32>,
    },
    Close { seat: Seat },
}

/// Signaling bound for a browser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignalOut {
    Answer { sdp: String },
    Candidate {
        candidate: String,
        sdp_mid: Option<String>,
        sdp_index: Option<u32>,
    },
    /// The `voice_ice_servers` blob from `config.toml`, relayed verbatim.
    IceServers { config: String },
    Error { message: String },
}

/// The seam between this plane and whoever terminates WebRTC sessions.
///
/// A data-plane owner makes exactly two calls: [`SignalHub::take_signals`] once,
/// and [`SignalHub::publish`] per seat as the handshake proceeds. Nothing else
/// in the control plane needs to know that a data plane exists.
#[derive(Clone)]
pub struct SignalHub {
    inbound_tx: Sender<SignalIn>,
    /// Handed over exactly once; a second consumer would make each one see half
    /// the offers.
    inbound_rx: Arc<std::sync::Mutex<Option<Receiver<SignalIn>>>>,
    routes: Arc<std::sync::Mutex<HashMap<String, RoutedQueue>>>,
    ice_config: Arc<String>,
    next_attach: Arc<AtomicU64>,
    delivered: Arc<AtomicU64>,
    /// Replies refused for any reason: no live route for that seat, its queue
    /// was full, or its socket vanished. Counted, never blocking.
    dropped_full: Arc<AtomicU64>,
}

struct RoutedQueue {
    /// Which socket generation owns this queue, so a reconnecting client's
    /// stale task cannot delete the route that replaced it.
    attach_id: u64,
    tx: Sender<SignalOut>,
}

impl SignalHub {
    pub fn new(ice_config: String) -> SignalHub {
        let (inbound_tx, inbound_rx) = mpsc_channel(INBOUND_QUEUE);
        SignalHub {
            inbound_tx,
            inbound_rx: Arc::new(std::sync::Mutex::new(Some(inbound_rx))),
            routes: Arc::new(std::sync::Mutex::new(HashMap::new())),
            ice_config: Arc::new(ice_config),
            next_attach: Arc::new(AtomicU64::new(1)),
            delivered: Arc::new(AtomicU64::new(0)),
            dropped_full: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn ice_config(&self) -> &str {
        &self.ice_config
    }

    /// The browser → data-plane queue. `None` if something already owns it.
    pub fn take_signals(&self) -> Option<Receiver<SignalIn>> {
        self.inbound_rx.lock().ok()?.take()
    }

    /// Open a reply queue for `seat`, called as the socket upgrades so an answer
    /// produced a microsecond later already has somewhere to go.
    pub fn attach(&self, seat: &str) -> SeatReplies {
        let (tx, rx) = mpsc_channel(PER_SEAT_QUEUE);
        let attach_id = self.next_attach.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut routes) = self.routes.lock() {
            routes.insert(seat.to_string(), RoutedQueue { attach_id, tx });
        }
        SeatReplies {
            rx,
            seat: seat.to_string(),
            attach_id,
            hub: self.clone(),
        }
    }

    /// Drop a seat's reply queue, and only if generation `attach_id` is still
    /// the live one.
    fn detach(&self, seat: &str, attach_id: u64) {
        if let Ok(mut routes) = self.routes.lock()
            && routes.get(seat).is_some_and(|q| q.attach_id == attach_id)
        {
            routes.remove(seat);
        }
    }

    /// Route one reply. `false` means the reply was dropped and counted: no
    /// socket is listening for that seat, its queue is full, or its socket
    /// vanished mid-publish — the browser will time out and reconnect.
    pub fn publish(&self, seat: &str, msg: SignalOut) -> bool {
        let Some(tx) = self
            .routes
            .lock()
            .ok()
            .and_then(|r| r.get(seat).map(|q| q.tx.clone()))
        else {
            self.dropped_full.fetch_add(1, Ordering::Relaxed);
            return false;
        };
        match tx.try_send(msg) {
            Ok(()) => {
                self.delivered.fetch_add(1, Ordering::Relaxed);
                true
            }
            Err(TrySendError::Full(_)) | Err(TrySendError::Closed(_)) => {
                self.dropped_full.fetch_add(1, Ordering::Relaxed);
                false
            }
        }
    }

    /// Queue one browser message for the data plane. Never blocks.
    ///
    /// `false` means nobody will read it: the inbound queue is full, or no data
    /// plane has claimed it yet. Refusing to *accept* while the queue is
    /// unclaimed is what lets the socket answer an `error` immediately instead
    /// of letting the browser wait out its own ICE timeout — and it is a read of
    /// the hand-over slot, not a claim, so a data plane that starts later still
    /// finds the queue waiting for it.
    pub fn post(&self, msg: SignalIn) -> bool {
        if self.inbound_unowned() {
            self.dropped_full.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        self.inbound_tx.try_send(msg).is_ok()
    }

    /// True while [`SignalHub::take_signals`] has never been called: the receiver
    /// is still sitting in its slot. `Close` is the one message the hub must
    /// attempt even then, because "the data plane started and then this socket
    /// went away" and "no data plane ever started" look identical here, and only
    /// an actual attempt can tell them apart.
    fn inbound_unowned(&self) -> bool {
        self.inbound_rx.lock().map(|slot| slot.is_some()).unwrap_or(false)
    }

    /// Attempt a `Close` regardless of who owns the queue. A data plane that
    /// claimed the inbound receiver gets exactly one notice per accepted socket;
    /// one that never existed drops the notice into a queue nobody reads, which
    /// is the correct outcome — nothing is waiting for the seat.
    fn try_post(&self, msg: SignalIn) -> bool {
        self.inbound_tx.try_send(msg).is_ok()
    }

    pub fn peers(&self) -> usize {
        self.routes.lock().map(|r| r.len()).unwrap_or(0)
    }

    pub fn counts(&self) -> (u64, u64) {
        (self.delivered.load(Relaxed), self.dropped_full.load(Relaxed))
    }
}

/// A socket's reply queue. Removing the seat's route happens on drop, so a
/// cancelled or panicking socket task cannot leave a dead queue behind.
pub struct SeatReplies {
    rx: Receiver<SignalOut>,
    seat: String,
    attach_id: u64,
    hub: SignalHub,
}

impl SeatReplies {
    /// The next reply for this seat, or `None` once every sender is gone.
    pub async fn next(&mut self) -> Option<SignalOut> {
        self.rx.recv().await
    }
}

impl Drop for SeatReplies {
    fn drop(&mut self) {
        self.hub.detach(&self.seat, self.attach_id);
    }
}

/// `pub fn router(&ControlState) -> Router<ControlState>` — frozen entry point.
pub fn router(_: &ControlState) -> Router<ControlState> {
    Router::new().route("/ws/signal", get(upgrade))
}

#[derive(Deserialize)]
pub struct SignalQuery {
    /// The `seatToken` from a matchmake reply. Required.
    seat: Option<String>,
}

async fn upgrade(
    State(state): State<ControlState>,
    Query(q): Query<SignalQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let seat = match q.seat.as_deref() {
        Some(token) => state.verify_seat(token),
        None => Err("a seat token from matchmake is required".to_string()),
    };
    match seat {
        // Resolved before the upgrade: an unauthenticated socket never becomes
        // a task that has to be cleaned up afterwards.
        Ok(seat) => ws.on_upgrade(move |socket| run_socket(socket, state, seat)),
        Err(reason) => crate::ApiError::new(StatusCode::UNAUTHORIZED, reason).into_response(),
    }
}

/// What a browser sends. `type` is the tag; absent fields are a protocol error
/// reported by name, not a blanket deserialize failure.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ClientFrame {
    Offer { sdp: String },
    Candidate {
        candidate: String,
        #[serde(rename = "sdpMid")]
        sdp_mid: Option<String>,
        // The WebRTC-native spelling first (`sdpMLineIndex`, what the SDK and
        // `RTCIceCandidateInit` produce); `sdpIndex` stays accepted so the
        // shape documented in earlier drafts does not break mid-rollout.
        #[serde(rename = "sdpMLineIndex", alias = "sdpIndex")]
        sdp_index: Option<u32>,
    },
    Close,
}

/// What this plane writes back. `ice.config` is emitted as JSON rather than as
/// a quoted string, so a page can read `msg.config` directly.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ServerFrame {
    Ice { config: serde_json::Value },
    Answer { sdp: String },
    Candidate {
        candidate: String,
        #[serde(rename = "sdpMid", skip_serializing_if = "Option::is_none")]
        sdp_mid: Option<String>,
        #[serde(rename = "sdpMLineIndex", skip_serializing_if = "Option::is_none")]
        sdp_index: Option<u32>,
    },
    Error { message: String },
}

impl From<SignalOut> for ServerFrame {
    fn from(msg: SignalOut) -> ServerFrame {
        match msg {
            SignalOut::Answer { sdp } => ServerFrame::Answer { sdp },
            SignalOut::Candidate { candidate, sdp_mid, sdp_index } => {
                ServerFrame::Candidate { candidate, sdp_mid, sdp_index }
            }
            SignalOut::IceServers { config } => ServerFrame::Ice { config: parse_ice(&config) },
            SignalOut::Error { message } => ServerFrame::Error { message },
        }
    }
}

/// Malformed ICE text becomes an empty list rather than a failed connection:
/// the config is operator-supplied, and LAN-only still works.
pub fn parse_ice(config: &str) -> serde_json::Value {
    serde_json::from_str(config).unwrap_or_else(|_| serde_json::Value::Array(vec![]))
}

/// Posts exactly one [`SignalIn::Close`] for its socket, however that socket's
/// task ends: a client `{"type":"close"}`, a WS Close frame, a socket error, a
/// send failure, or a cancelled future. `post` is a non-blocking `try_send`,
/// which is what makes a `Drop`-time notice legal on this plane.
struct CloseGuard {
    hub: SignalHub,
    seat: Seat,
}

impl Drop for CloseGuard {
    fn drop(&mut self) {
        let _ = self.hub.try_post(SignalIn::Close { seat: self.seat.clone() });
    }
}

async fn run_socket(mut socket: WebSocket, state: ControlState, seat: Seat) {
    let mut replies = state.signals.attach(&seat.id);
    // Declared after `replies`, so it drops first: the data plane sees the
    // seat close while its reply route is still addressable, and the route
    // removal that follows is generation-checked.
    let _close_guard = CloseGuard { hub: state.signals.clone(), seat: seat.clone() };

    // The client cannot gather relay candidates without the ICE list, so this
    // plane publishes it to the seat — through the same hub the data plane
    // answers on — before anything else can reach the socket. The queue is
    // freshly attached and empty, so this publish cannot fail while owned.
    let _ = state.signals.publish(
        &seat.id,
        SignalOut::IceServers { config: state.signals.ice_config().to_string() },
    );

    loop {
        tokio::select! {
            // biased: an inbound offer must not queue behind a burst of our own
            // candidates when both are ready in the same tick.
            biased;

            inbound = socket.next() => match inbound {
                Some(Ok(Message::Text(text))) => {
                    match serde_json::from_str::<ClientFrame>(text.as_str()) {
                        Ok(ClientFrame::Offer { sdp }) => {
                            if !state.signals.post(SignalIn::Offer { seat: seat.clone(), sdp }) {
                                let _ = send_frame(&mut socket, &ServerFrame::Error {
                                    message: "data plane is not accepting signaling".into(),
                                }).await;
                            }
                        }
                        Ok(ClientFrame::Candidate { candidate, sdp_mid, sdp_index }) => {
                            if !state.signals.post(SignalIn::Candidate {
                                seat: seat.clone(), candidate, sdp_mid, sdp_index,
                            }) {
                                let _ = send_frame(&mut socket, &ServerFrame::Error {
                                    message: "data plane is not accepting signaling".into(),
                                }).await;
                            }
                        }
                        // The client says it is done: end the session. The
                        // single Close it owes the data plane is posted on
                        // teardown, not here — otherwise a graceful close
                        // would be a duplicate.
                        Ok(ClientFrame::Close) => break,
                        Err(e) => {
                            // A bad frame is the client's bug, not a reason to
                            // drop the session: name it and keep going.
                            let _ = send_frame(&mut socket, &ServerFrame::Error {
                                message: format!("malformed signaling frame: {e}"),
                            }).await;
                        }
                    }
                }
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                // Ping/Pong are answered by axum; binary frames are not part of
                // this protocol and are ignored rather than misparsed.
                Some(Ok(_)) => {}
            },

            reply = replies.next() => match reply {
                Some(msg) => {
                    if send_frame(&mut socket, &msg.into()).await.is_err() {
                        break;
                    }
                }
                // Every sender is gone, which for a live socket means the hub
                // was dropped with the plane shutting down.
                None => break,
            },
        }
    }
}

async fn send_frame(socket: &mut WebSocket, frame: &ServerFrame) -> Result<(), axum::Error> {
    let json = serde_json::to_string(frame).expect("ServerFrame is always serializable");
    socket.send(Message::Text(json.into())).await
}

fn mpsc_channel<T>(cap: usize) -> (Sender<T>, Receiver<T>) {
    tokio::sync::mpsc::channel(cap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::path::Path;
    use std::time::{Duration, Instant};

    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    use engine_bus::Bus;
    use engine_config::{ControlConfig, GameConfig};
    use crate::OnceReceiver;
    use crate::rate_limit::IpRateLimiter;
    use crate::seats::SeatRegistry;
    use crate::static_routes::StaticServer;

    /// Every socket/queue await in these tests is bounded by this. A signaling
    /// relay that does not answer in two seconds is broken, not slow.
    const AWAIT_TIMEOUT: Duration = Duration::from_secs(2);

    fn test_state() -> (ControlState, Receiver<SignalIn>) {
        let game = GameConfig {
            voice_ice_servers: r#"[{"urls":["stun:stun.l.google.com:19302"]}]"#.into(),
            ..Default::default()
        };
        let control = ControlConfig {
            seat_secret: "test-seat-secret".into(),
            ..Default::default()
        };
        let bus = Bus::new(16, 16);
        let state = ControlState {
            control: Arc::new(control),
            game: Arc::new(game.clone()),
            bus: bus.clone(),
            seats: Arc::new(std::sync::Mutex::new(SeatRegistry::new("test-seat-secret"))),
            limiter: Arc::new(std::sync::Mutex::new(IpRateLimiter::new(1000, 1000))),
            signals: SignalHub::new(game.voice_ice_servers.clone()),
            statics: Arc::new(StaticServer::new(Path::new("."), &game)),
            port: 0,
            status_rx: Arc::new(OnceReceiver::new(bus.status_rx.clone())),
            latest_status: Arc::new(std::sync::Mutex::new(None)),
            started: Instant::now(),
        };
        // The test plays the data plane: it owns the inbound queue, exactly
        // like the future str0m session task would.
        let inbound =
            state.signals.take_signals().expect("a fresh hub hands over its inbound queue");
        (state, inbound)
    }

    async fn spawn_plane(state: &ControlState) -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = router(state).with_state(state.clone());
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        addr
    }

    // A hand-written RFC 6455 client: masked requests, unmasked replies. No WS
    // dependency exists in this crate's dev-deps, and the endpoint under test
    // is exactly a socket, so a raw one proves more than a mock would.

    struct WsClient {
        io: TcpStream,
    }

    /// The RFC 6455 sample key; the server's reply to it is never checked here.
    const WS_KEY: &str = "dGhlIHNhbXBsZSBub25jZQ==";

    fn upgrade_request(path: &str, addr: SocketAddr) -> String {
        format!(
            "GET {path} HTTP/1.1\r\nHost: {addr}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
             Sec-WebSocket-Key: {WS_KEY}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        )
    }

    async fn connect_io(addr: SocketAddr, path: &str) -> TcpStream {
        let mut io = tokio::time::timeout(AWAIT_TIMEOUT, TcpStream::connect(addr))
            .await
            .expect("connect timed out")
            .expect("connect failed");
        let req = upgrade_request(path, addr);
        tokio::time::timeout(AWAIT_TIMEOUT, io.write_all(req.as_bytes()))
            .await
            .expect("write timed out")
            .expect("write failed");
        io
    }

    async fn read_byte(io: &mut TcpStream) -> Option<u8> {
        tokio::time::timeout(AWAIT_TIMEOUT, io.read_u8())
            .await
            .expect("socket stalled")
            .ok()
    }

    async fn read_chunk(io: &mut TcpStream, buf: &mut [u8]) -> Option<()> {
        tokio::time::timeout(AWAIT_TIMEOUT, io.read_exact(buf))
            .await
            .expect("socket stalled")
            .ok()?;
        Some(())
    }

    /// Read response headers up to the blank line.
    async fn read_head(io: &mut TcpStream) -> Option<String> {
        let mut head = String::new();
        loop {
            head.push(read_byte(io).await? as char);
            if head.ends_with("\r\n\r\n") {
                return Some(head);
            }
        }
    }

    impl WsClient {
        /// Open a signaling session. Panics (fails the test) unless the
        /// server answered 101.
        async fn open(addr: SocketAddr, path: &str) -> WsClient {
            let mut io = connect_io(addr, path).await;
            let head = read_head(&mut io).await.expect("no response head");
            assert!(head.starts_with("HTTP/1.1 101"), "upgrade refused: {head}");
            WsClient { io }
        }

        async fn send_frame(&mut self, opcode: u8, payload: &[u8]) {
            let mut frame = vec![0x80 | opcode];
            if payload.len() < 126 {
                frame.push(0x80 | payload.len() as u8);
            } else if payload.len() < (1 << 16) {
                frame.push(0x80 | 126);
                frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
            } else {
                frame.push(0x80 | 127);
                frame.extend_from_slice(&(payload.len() as u64).to_be_bytes());
            }
            let mask = [0x37u8, 0x38, 0x39, 0x3A];
            frame.extend_from_slice(&mask);
            frame.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
            tokio::time::timeout(AWAIT_TIMEOUT, self.io.write_all(&frame))
                .await
                .expect("write timed out")
                .expect("write failed");
        }

        async fn send_text(&mut self, msg: &str) {
            self.send_frame(0x1, msg.as_bytes()).await;
        }

        /// The next text frame, or `None` once the server closes or hangs up.
        /// Pings are ponged; control noise is skipped.
        async fn recv(&mut self) -> Option<String> {
            loop {
                let b0 = read_byte(&mut self.io).await?;
                let b1 = read_byte(&mut self.io).await?;
                assert_eq!(b1 & 0x80, 0, "the server must not mask its frames");
                let opcode = b0 & 0x0f;
                let mut len = (b1 & 0x7f) as u64;
                if len == 126 {
                    let mut x = [0u8; 2];
                    read_chunk(&mut self.io, &mut x).await?;
                    len = u16::from_be_bytes(x) as u64;
                } else if len == 127 {
                    let mut x = [0u8; 8];
                    read_chunk(&mut self.io, &mut x).await?;
                    len = u64::from_be_bytes(x);
                    assert!(len < 1 << 20, "absurd frame length {len}");
                }
                let mut payload = vec![0u8; len as usize];
                if len > 0 {
                    read_chunk(&mut self.io, &mut payload).await?;
                }
                match opcode {
                    0x1 => return String::from_utf8(payload).ok(),
                    0x8 => return None,
                    0x9 => self.send_frame(0xA, &payload).await,
                    _ => {}
                }
            }
        }

        async fn recv_json(&mut self) -> serde_json::Value {
            serde_json::from_str(&self.recv().await.expect("expected a text frame")).unwrap()
        }
    }

    /// A complete HTTP GET carrying upgrade headers, read as a plain response:
    /// status line and `Content-Length`-sized body. Used by the refusal test.
    async fn upgrade_get(addr: SocketAddr, path: &str) -> (u16, serde_json::Value) {
        let mut io = connect_io(addr, path).await;
        let head = read_head(&mut io).await.expect("no response head");
        let status: u16 =
            head.split_whitespace().nth(1).expect("status line").parse().expect("numeric status");
        let len = head
            .lines()
            .find_map(|l| {
                let (k, v) = l.split_once(':')?;
                if k.trim().eq_ignore_ascii_case("content-length") {
                    v.trim().parse().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0usize);
        let mut body = vec![0u8; len];
        if len > 0 {
            read_chunk(&mut io, &mut body).await.expect("body shorter than Content-Length");
        }
        let json = serde_json::from_slice(&body)
            .unwrap_or_else(|e| panic!("error body is not JSON: {e}"));
        (status, json)
    }

    async fn next_signal(rx: &mut Receiver<SignalIn>) -> SignalIn {
        tokio::time::timeout(AWAIT_TIMEOUT, rx.recv())
            .await
            .expect("the data-plane queue never drained")
            .expect("inbound queue closed")
    }

    /// Nothing further may arrive for a while: this is the "no duplicate"
    /// half of the exactly-once Close contract.
    async fn expect_quiet(rx: &mut Receiver<SignalIn>) {
        assert!(
            tokio::time::timeout(Duration::from_millis(250), rx.recv()).await.is_err(),
            "signaling sent the data plane more than it was owed"
        );
    }

    /// Route removal races the socket's close-frame write only by microns, so
    /// poll instead of assuming ordering.
    async fn wait_peers(hub: &SignalHub, want: usize) {
        for _ in 0..100 {
            if hub.peers() == want {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("peers() never reached {want} (still {})", hub.peers());
    }

    fn signal_path(seat: &Seat) -> String {
        format!("/ws/signal?seat={}", seat.token)
    }

    #[tokio::test]
    async fn relays_offer_and_answer_over_a_real_socket() {
        let (state, mut inbound) = test_state();
        let addr = spawn_plane(&state).await;
        let seat = state
            .seats
            .lock()
            .unwrap()
            .mint("tank-3d", 300)
            .expect("an empty registry mints a valid room");
        let mut ws = WsClient::open(addr, &signal_path(&seat)).await;

        // ICE config is the first frame: a client cannot gather without it.
        assert_eq!(
            ws.recv_json().await,
            json!({ "type": "ice", "config": [{ "urls": ["stun:stun.l.google.com:19302"] }] })
        );

        // Offer → data plane, seat stamped by the handler, not by the body.
        ws.send_text(r#"{"type":"offer","sdp":"v=0 fake offer"}"#).await;
        assert_eq!(
            next_signal(&mut inbound).await,
            SignalIn::Offer { seat: seat.clone(), sdp: "v=0 fake offer".into() }
        );

        // Answer → browser, through the hub exactly as a data plane would.
        assert!(
            state
                .signals
                .publish(&seat.id, SignalOut::Answer { sdp: "v=0 fake answer".into() })
        );
        assert_eq!(
            ws.recv_json().await,
            json!({ "type": "answer", "sdp": "v=0 fake answer" })
        );

        // Candidates both ways; the inbound alias `sdpIndex` maps to the same
        // field as `sdpMLineIndex`.
        ws.send_text(
            r#"{"type":"candidate","candidate":"candidate:1 1 udp 1 10.0.0.1 5000 typ host","sdpMid":"0","sdpMLineIndex":0}"#,
        )
        .await;
        assert_eq!(
            next_signal(&mut inbound).await,
            SignalIn::Candidate {
                seat: seat.clone(),
                candidate: "candidate:1 1 udp 1 10.0.0.1 5000 typ host".into(),
                sdp_mid: Some("0".into()),
                sdp_index: Some(0),
            }
        );
        assert!(state.signals.publish(
            &seat.id,
            SignalOut::Candidate {
                candidate: "candidate:2 1 udp 2 10.0.0.2 5001 typ host".into(),
                sdp_mid: Some("0".into()),
                sdp_index: Some(0),
            }
        ));
        assert_eq!(
            ws.recv().await.unwrap(),
            r#"{"type":"candidate","candidate":"candidate:2 1 udp 2 10.0.0.2 5001 typ host","sdpMid":"0","sdpMLineIndex":0}"#
        );
        assert_eq!(
            state.signals.counts(),
            (3, 0),
            "ice, answer and candidate all arrived through the hub"
        );

        // A client `{"type":"close"}` costs the data plane exactly one Close.
        ws.send_text(r#"{"type":"close"}"#).await;
        assert_eq!(next_signal(&mut inbound).await, SignalIn::Close { seat: seat.clone() });
        expect_quiet(&mut inbound).await;
        wait_peers(&state.signals, 0).await;
        assert!(
            !state.signals.publish(&seat.id, SignalOut::Error { message: "gone".into() }),
            "a seat whose socket closed must have no route left"
        );
    }

    #[tokio::test]
    async fn refuses_a_bad_or_missing_seat_token_before_the_upgrade() {
        let (state, mut inbound) = test_state();
        let addr = spawn_plane(&state).await;
        for path in
            ["/ws/signal", "/ws/signal?seat=not-a-signed-token", "/ws/signal?seat."]
        {
            let (status, body) = upgrade_get(addr, path).await;
            assert_eq!(status, 401, "{path} was not refused with 401");
            assert!(
                body["error"].as_str().unwrap_or("").contains("seat"),
                "plane-wide error body missing: {body}"
            );
        }
        assert_eq!(state.signals.peers(), 0, "a refused upgrade must never become a task");
        expect_quiet(&mut inbound).await;
    }

    #[tokio::test]
    async fn a_reconnect_supersedes_the_old_socket_without_losing_its_route() {
        let (state, mut inbound) = test_state();
        let addr = spawn_plane(&state).await;
        let seat = state
            .seats
            .lock()
            .unwrap()
            .mint("tank-3d", 300)
            .expect("an empty registry mints a valid room");
        let path = signal_path(&seat);

        let mut first = WsClient::open(addr, &path).await;
        assert_eq!(first.recv_json().await["type"], "ice");
        let mut second = WsClient::open(addr, &path).await;
        assert_eq!(second.recv_json().await["type"], "ice");
        // One route per seat: the newer attach owns it.
        assert_eq!(state.signals.peers(), 1);

        // The old socket tears down — its SeatReplies::Drop runs detach with a
        // stale generation and must NOT delete the second socket's route.
        first.send_text(r#"{"type":"close"}"#).await;
        assert_eq!(next_signal(&mut inbound).await, SignalIn::Close { seat: seat.clone() });
        assert!(
            state.signals.publish(&seat.id, SignalOut::Answer { sdp: "still here".into() }),
            "the stale socket's teardown deleted the live route"
        );
        assert_eq!(second.recv_json().await, json!({ "type": "answer", "sdp": "still here" }));

        // And the replacement socket is owed its own single Close.
        second.send_text(r#"{"type":"close"}"#).await;
        assert_eq!(next_signal(&mut inbound).await, SignalIn::Close { seat: seat.clone() });
        expect_quiet(&mut inbound).await;
        wait_peers(&state.signals, 0).await;
    }

    #[tokio::test]
    async fn a_dropped_socket_still_teaches_the_data_plane_once() {
        // No `{"type":"close"}`, no Close frame: just the TCP connection going
        // away. The guard, not the loop, owns the notice.
        let (state, mut inbound) = test_state();
        let addr = spawn_plane(&state).await;
        let seat = state
            .seats
            .lock()
            .unwrap()
            .mint("tank-3d", 300)
            .expect("an empty registry mints a valid room");
        let mut ws = WsClient::open(addr, &signal_path(&seat)).await;
        assert_eq!(ws.recv_json().await["type"], "ice");
        drop(ws);
        assert_eq!(next_signal(&mut inbound).await, SignalIn::Close { seat });
        expect_quiet(&mut inbound).await;
        wait_peers(&state.signals, 0).await;
    }

    #[tokio::test]
    async fn a_malformed_frame_errors_without_killing_the_session() {
        let (state, mut inbound) = test_state();
        let addr = spawn_plane(&state).await;
        let seat = state
            .seats
            .lock()
            .unwrap()
            .mint("tank-3d", 300)
            .expect("an empty registry mints a valid room");
        let mut ws = WsClient::open(addr, &signal_path(&seat)).await;
        assert_eq!(ws.recv_json().await["type"], "ice");

        ws.send_text("this is not json").await;
        let err = ws.recv_json().await;
        assert_eq!(err["type"], "error");
        assert!(err["message"].as_str().unwrap().contains("malformed"));

        // The session must still be live: a real offer reaches the data plane,
        // and a candidate using the legacy `sdpIndex` alias parses too.
        ws.send_text(r#"{"type":"offer","sdp":"v=0 still alive"}"#).await;
        assert_eq!(
            next_signal(&mut inbound).await,
            SignalIn::Offer { seat: seat.clone(), sdp: "v=0 still alive".into() }
        );
        ws.send_text(
            r#"{"type":"candidate","candidate":"candidate:3 1 udp 3 h 9 typ host","sdpIndex":1}"#,
        )
        .await;
        match next_signal(&mut inbound).await {
            SignalIn::Candidate { sdp_mid, sdp_index, .. } => {
                assert_eq!(sdp_mid, None);
                assert_eq!(sdp_index, Some(1), "the sdpIndex alias must parse");
            }
            other => panic!("expected a candidate, got {other:?}"),
        }
        ws.send_text(r#"{"type":"close"}"#).await;
        assert_eq!(next_signal(&mut inbound).await, SignalIn::Close { seat });
    }

    #[test]
    fn parse_ice_is_lenient_because_the_blob_is_operator_supplied() {
        let good = parse_ice(r#"[{"urls":["stun:a"]},{"urls":["turn:b"],"username":"u"}]"#);
        assert_eq!(good.as_array().unwrap().len(), 2, "a valid list is relayed verbatim");
        assert_eq!(parse_ice("{not json"), serde_json::Value::Array(vec![]));
        assert_eq!(parse_ice(""), serde_json::Value::Array(vec![]));
    }

    #[test]
    fn server_frames_match_the_documented_shapes() {
        assert_eq!(
            serde_json::to_string(&ServerFrame::from(SignalOut::IceServers {
                config: r#"[{"urls":["stun:a"]}]"#.into()
            }))
            .unwrap(),
            r#"{"type":"ice","config":[{"urls":["stun:a"]}]}"#
        );
        assert_eq!(
            serde_json::to_string(&ServerFrame::from(SignalOut::Answer { sdp: "v=0".into() }))
                .unwrap(),
            r#"{"type":"answer","sdp":"v=0"}"#
        );
        assert_eq!(
            serde_json::to_string(&ServerFrame::from(SignalOut::Candidate {
                candidate: "c".into(),
                sdp_mid: None,
                sdp_index: None,
            }))
            .unwrap(),
            r#"{"type":"candidate","candidate":"c"}"#
        );
        assert_eq!(
            serde_json::to_string(&ServerFrame::from(SignalOut::Error {
                message: "boom".into()
            }))
            .unwrap(),
            r#"{"type":"error","message":"boom"}"#
        );
    }

    #[test]
    fn publish_to_an_unknown_seat_is_refused_and_counted() {
        let hub = SignalHub::new("[]".into());
        assert!(!hub.publish("nobody", SignalOut::Error { message: "x".into() }));
        assert_eq!(hub.counts(), (0, 1));
        assert_eq!(hub.peers(), 0);
    }

    #[test]
    fn a_full_per_seat_queue_drops_and_counts_without_blocking_the_publisher() {
        let hub = SignalHub::new("[]".into());
        let mut replies = hub.attach("seat");
        for i in 0..PER_SEAT_QUEUE {
            assert!(hub.publish("seat", SignalOut::Answer { sdp: i.to_string() }));
        }
        assert_eq!(hub.counts(), (PER_SEAT_QUEUE as u64, 0));
        assert!(!hub.publish("seat", SignalOut::Answer { sdp: "overflow".into() }));
        assert_eq!(hub.counts(), (PER_SEAT_QUEUE as u64, 1), "a full queue drops and counts");
        // Freeing one slot admits exactly one more reply.
        assert_eq!(replies.rx.blocking_recv().unwrap(), SignalOut::Answer { sdp: "0".into() });
        assert!(hub.publish("seat", SignalOut::Answer { sdp: "again".into() }));
        assert_eq!(hub.counts(), ((PER_SEAT_QUEUE + 1) as u64, 1));
    }

    #[test]
    fn attach_replaces_the_route_and_detach_respects_generation() {
        let hub = SignalHub::new("[]".into());
        let first = hub.attach("seat");
        let _second = hub.attach("seat");
        assert_eq!(hub.peers(), 1, "one route per seat");
        drop(first); // stale generation: must not remove the live route
        assert_eq!(hub.peers(), 1);
        assert!(hub.publish("seat", SignalOut::Answer { sdp: "v=0".into() }));
        assert_eq!(hub.counts(), (1, 0));
        drop(_second); // current generation: tears down
        assert_eq!(hub.peers(), 0);
        assert!(!hub.publish("seat", SignalOut::Answer { sdp: "v=0".into() }));
        assert_eq!(hub.counts(), (1, 1));
    }

    #[test]
    fn the_inbound_queue_is_bounded_and_post_never_blocks() {
        let hub = SignalHub::new("[]".into());
        let _rx = hub.take_signals().expect("first hand-over succeeds");
        assert!(hub.take_signals().is_none(), "the inbound queue is handed over exactly once");
        let seat =
            Seat { id: "s".into(), room: "r".into(), token: "t".into(), expires_at: u64::MAX };
        for _ in 0..INBOUND_QUEUE {
            assert!(hub.post(SignalIn::Offer { seat: seat.clone(), sdp: "v=0".into() }));
        }
        assert!(!hub.post(SignalIn::Offer { seat, sdp: "one too many".into() }));
    }
}
