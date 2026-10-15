//! `matchmake` — seat reservation, the first thing every client does.
//!
//! Same `ISeatReservation` JSON body as the in-transport implementation, so an
//! existing page works against either port, plus two fields a client needs to
//! get onto the data planes: `seatToken` (the HMAC token the raw-UDP plane
//! authenticates with) and `gamePort` (where the game WebSocket lives, which is
//! *not* this port once the control plane is separate).
//!
//! Two things make this the security boundary it has to be: every seat is
//! signed, and every request is charged to the socket's IP.
//!
//! ## The ordering that matters
//!
//! ```text
//! charge the per-IP bucket  ->  validate method/room  ->  HMAC work  ->  map write
//! ```
//!
//! The bucket is taken *first*, before any signature is computed and before the
//! seat map grows. That is the endpoint's whole abuse model: a client spoofing
//! source addresses should cost a hash lookup per packet, not an HMAC-SHA256
//! plus a heap insertion. `reserve` is therefore a plain function of
//! `(ip, method, room, body)`, so a test can prove the cost was never paid by
//! watching the registry's mint counter across a refusal.
//!
//! ## Spoofing
//!
//! A seat's room comes from the *signature*, never from the URL. On
//! `reconnect`/`rejoin` the URL's room segment is still validated for shape
//! (it is echoed and logged), but the resumed seat keeps the room it was minted
//! for: editing the path cannot move a client into someone else's room.

use std::net::{IpAddr, SocketAddr};

use axum::Router;
use axum::extract::{ConnectInfo, DefaultBodyLimit, Path as UrlPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;

use crate::ApiError;
use crate::ControlState;
use crate::seats::Seat;
use crate::seats::SeatRegistry;

/// The browser client's method whitelist. Anything else is a room handler the
/// engine does not have.
pub const MATCHMAKE_METHODS: &[&str] =
    &["joinOrCreate", "join", "joinById", "create", "rejoin", "reconnect"];

/// A matchmake body is a seat ticket, at most a few hundred bytes. Anything
/// larger is not this endpoint's traffic — and the ceiling is charged *after*
/// the bucket, so a flood still costs only a hash lookup.
pub const MAX_BODY_BYTES: usize = 4096;

/// Room names land in a URL, a log line, a JSON string and the sim plane's room
/// map. Keep them boring.
pub fn valid_room_name(room: &str) -> bool {
    (1..=64).contains(&room.len())
        && room
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// `pub fn router(&ControlState) -> Router<ControlState>` — frozen entry point.
///
/// `/matchmake/…` is the path the client already posts to; `/api/matchmake/…`
/// is the documented one. Both reach the same handler, so a page does not have
/// to know which phase it is talking to.
pub fn router(state: &ControlState) -> Router<ControlState> {
    let _ = state;
    Router::new()
        .route("/api/matchmake/{method}/{room}", post(handle))
        .route("/matchmake/{method}/{room}", post(handle))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
}

async fn handle(
    State(state): State<ControlState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    UrlPath((method, room)): UrlPath<(String, String)>,
    body: String,
) -> Result<Response, ApiError> {
    Ok(reserve(&state, peer.ip(), &method, &room, &body)?.into_response())
}

/// The endpoint, as a function of the socket's source address.
///
/// `pub` so the rate-limit-before-crypto property can be tested directly: a
/// loopback client can only ever present one IP, so the multi-source rule needs
/// the address handed in.
///
/// NOTE: `ConnectInfo<SocketAddr>` is only populated when the plane is served
/// with `into_make_service_with_connect_info::<SocketAddr>()`.
pub fn reserve(
    state: &ControlState,
    ip: IpAddr,
    method: &str,
    room: &str,
    body: &str,
) -> Result<SeatResponse, ApiError> {
    // 1. Charge this IP *before* anything expensive runs. An HMAC and a map
    //    insert are what a flood is really paying for, and a spoofed source
    //    address is only good for one bucket. A poisoned lock fails open: the
    //    limiter is defence in depth, the seat signature is the real gate, and
    //    locking the lobby is not a recovery plan.
    let charged = state
        .limiter
        .lock()
        .map(|mut l| l.allow(ip))
        .unwrap_or(true);
    if !charged {
        // `Retry-After` comes out of the same bucket that just said no, so a
        // page that waits the advertised time is not refused again.
        let wait = state
            .limiter
            .lock()
            .map(|l| l.retry_after(ip))
            .unwrap_or(1);
        return Err(ApiError::rate_limited(wait));
    }

    // 2. Cheap validation, still before any MAC work.
    if !MATCHMAKE_METHODS.contains(&method) {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            format!("no room handler for method '{}'", printable(method)),
        ));
    }
    if !valid_room_name(room) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "invalid room name: use 1-64 letters, digits, '-' or '_'",
        ));
    }

    // 3. `reconnect` and `rejoin` consume an existing token instead of minting
    //    a seat: the client sends its `reconnectionToken` back and gets the same
    //    seat — same `sessionId`, same room — so the WebSocket upgrade query and
    //    the raw-UDP `JOIN` both survive a reload.
    if method == "reconnect" || method == "rejoin" {
        return resume_seat(state, body);
    }

    // 4. `joinOrCreate` / `join` / `joinById` / `create` all mint: the room host
    //    owns membership, this plane only signs the seat.
    let seat = state
        .seats
        .lock()
        .map(|mut r: std::sync::MutexGuard<'_, SeatRegistry>| {
            r.mint(room, state.control.seat_ttl_secs)
        })
        .map_err(|_| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "seat registry unavailable"))?
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "the server is at its outstanding-seat limit",
            )
        })?;

    Ok(SeatResponse::new(state, &seat))
}

/// The client re-POSTs `reconnect` with its `reconnectionToken`; the seat it
/// belongs to is handed back unchanged, so room membership survives a reload.
fn resume_seat(state: &ControlState, body: &str) -> Result<SeatResponse, ApiError> {
    let token = json_string_field(body, "reconnectionToken")
        .ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "missing reconnectionToken"))?;
    let seat = state
        .seats
        .lock()
        .map(|mut r| r.verify(&token))
        .map_err(|_| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "seat registry unavailable"))?
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::UNAUTHORIZED,
                "reconnection token expired or unknown",
            )
        })?;
    Ok(SeatResponse::new(state, &seat))
}

/// `ISeatReservation` plus the data-plane fields. Field order is irrelevant to
/// `JSON.parse`; the values must be escaped, because `room` reached us through
/// a URL and is echoed straight back.
pub struct SeatResponse {
    pub name: String,
    pub session_id: String,
    pub room_id: String,
    pub process_id: String,
    pub reconnection_token: String,
    pub seat_token: String,
    pub game_port: u16,
    pub control_port: u16,
    pub ice_servers: String,
    pub expires_at: u64,
}

impl SeatResponse {
    fn new(state: &ControlState, seat: &Seat) -> SeatResponse {
        SeatResponse {
            name: seat.room.clone(),
            session_id: seat.id.clone(),
            room_id: seat.room.clone(),
            process_id: format!("openoxide_{}", std::process::id()),
            reconnection_token: seat.token.clone(),
            seat_token: seat.token.clone(),
            game_port: state.game.port,
            control_port: state.control_port(),
            ice_servers: state.signals.ice_config().to_string(),
            expires_at: seat.expires_at,
        }
    }

    pub fn to_json(&self) -> String {
        format!(
            concat!(
                "{{\"name\":\"{name}\",\"sessionId\":\"{sid}\",\"roomId\":\"{room}\",",
                "\"processId\":\"{proc}\",\"reconnectionToken\":\"{token}\",",
                "\"seatToken\":\"{seat}\",\"gamePort\":{gport},\"controlPort\":{cport},",
                "\"expiresAt\":{exp},\"iceServers\":{ice}}}"
            ),
            name = json_escape(&self.name),
            sid = json_escape(&self.session_id),
            room = json_escape(&self.room_id),
            proc = json_escape(&self.process_id),
            token = json_escape(&self.reconnection_token),
            seat = json_escape(&self.seat_token),
            gport = self.game_port,
            cport = self.control_port,
            exp = self.expires_at,
            ice = json_string_literal(&self.ice_servers),
        )
    }
}

impl IntoResponse for SeatResponse {
    fn into_response(self) -> Response {
        (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            self.to_json(),
        )
            .into_response()
    }
}

fn json_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// A JSON string literal for a value that is itself JSON text (the ICE server
/// blob), so the client parses the outer object and then the inner one.
fn json_string_literal(value: &str) -> String {
    format!("\"{}\"", json_escape(value))
}

/// Pull one string field out of a JSON body. Matchmake bodies are small and
/// flat, and a malformed one must be a 400 from the caller, not a panic here.
pub fn json_string_field(body: &str, key: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(body).ok()?;
    value.get(key)?.as_str().map(str::to_string)
}

/// Error text goes into a JSON string, and the plane's `ApiError` formatter
/// quotes with Rust's `Debug`, which renders a control character as `\u{1}` —
/// not valid JSON. Since the method name arrives percent-decoded from a URL,
/// those bytes are dropped here rather than echoed.
fn printable(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .take(64)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_bus::Bus;
    use engine_config::{ControlConfig, GameConfig};
    use crate::rate_limit::IpRateLimiter;
    use engine_config::DEV_SEAT_SECRET;
    use crate::signaling::SignalHub;
    use crate::{OnceReceiver, static_routes::StaticServer};
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    /// A `ControlState` with everything the matchmake path touches: real config
    /// defaults, a real bus, an empty static server.
    fn test_state(rps: u32, burst: u32) -> ControlState {
        test_state_with(rps, burst, "[{\"urls\":[\"stun:stun.l.google.com:19302\"]}]")
    }

    fn test_state_with(rps: u32, burst: u32, ice: &str) -> ControlState {
        let game = GameConfig::default();
        let control = ControlConfig {
            api_port: 0,
            seat_secret: DEV_SEAT_SECRET.to_string(),
            seat_ttl_secs: 300,
            rate_limit_rps: rps,
            rate_limit_burst: burst,
            ..ControlConfig::default()
        };
        let bus = Bus::new(64, 64);
        ControlState {
            control: Arc::new(control),
            game: Arc::new(game),
            bus: bus.clone(),
            seats: Arc::new(Mutex::new(SeatRegistry::new(DEV_SEAT_SECRET))),
            limiter: Arc::new(Mutex::new(IpRateLimiter::new(rps, burst))),
            signals: SignalHub::new(ice.to_string()),
            statics: Arc::new(StaticServer::new(Path::new("."), &GameConfig::default())),
            port: 8124,
            status_rx: Arc::new(OnceReceiver::new(bus.status_rx.clone())),
            latest_status: Arc::new(Mutex::new(None)),
            started: Instant::now(),
        }
    }

    fn ip(n: u8) -> IpAddr {
        IpAddr::from([198, 51, 100, n])
    }

    fn minted(state: &ControlState) -> u64 {
        state.seats.lock().unwrap().counts().0
    }

    fn ok(state: &ControlState, ip: IpAddr, method: &str, room: &str, body: &str) -> SeatResponse {
        match reserve(state, ip, method, room, body) {
            Ok(seat) => seat,
            Err(e) => panic!("{method}/{room} refused: {} {}", e.status, e.message),
        }
    }

    fn err(state: &ControlState, ip: IpAddr, method: &str, room: &str, body: &str) -> ApiError {
        match reserve(state, ip, method, room, body) {
            Ok(seat) => panic!("{method}/{room} should have been refused, minted {}", seat.session_id),
            Err(e) => e,
        }
    }

    fn reserve_ok(state: &ControlState, method: &str, room: &str, body: &str) -> SeatResponse {
        ok(state, ip(1), method, room, body)
    }

    fn now_unix() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
    }

    // ---- the JSON the SDK actually consumes --------------------------------

    #[test]
    fn a_reservation_carries_every_field_the_sdk_needs() {
        let state = test_state(1000, 1000);
        let seat = reserve_ok(&state, "joinOrCreate", "tank-3d", "{}");
        let json = seat.to_json();
        let v: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        for field in ["name", "sessionId", "roomId", "processId", "reconnectionToken"] {
            assert!(
                v.get(field).and_then(|f| f.as_str()).is_some(),
                "ISeatReservation is missing {field}: {json}"
            );
        }
        for field in ["seatToken", "gamePort", "controlPort", "expiresAt", "iceServers"] {
            assert!(v.get(field).is_some(), "data-plane field {field} missing: {json}");
        }
        assert_eq!(v["name"].as_str().unwrap(), "tank-3d");
        assert_eq!(v["roomId"].as_str().unwrap(), "tank-3d");
        assert_eq!(v["gamePort"].as_u64().unwrap(), u64::from(state.game.port));
        assert_eq!(v["controlPort"].as_u64().unwrap(), 8124);
        assert!(v["processId"].as_str().unwrap().starts_with("openoxide_"));
        assert_eq!(v["seatToken"].as_str(), v["reconnectionToken"].as_str());
        assert!(v["expiresAt"].as_u64().unwrap() > now_unix());
        assert!(!v["sessionId"].as_str().unwrap().is_empty(), "sessionId identifies the seat");
        // The ICE blob is a JSON *string* containing JSON — the shape the
        // in-transport matchmake emitted, so a client parses it a second time.
        let ice = v["iceServers"].as_str().expect("iceServers is a string");
        let parsed: serde_json::Value = serde_json::from_str(ice).unwrap();
        assert_eq!(parsed[0]["urls"][0].as_str().unwrap(), "stun:stun.l.google.com:19302");
    }

    #[test]
    fn every_mint_method_gets_a_seat() {
        let state = test_state(1000, 1000);
        for method in ["joinOrCreate", "join", "joinById", "create"] {
            let seat = reserve_ok(&state, method, "arena_final", "{}");
            assert_eq!(seat.room_id, "arena_final", "{method}");
            assert!(seat.seat_token.contains('.'), "{method} signed");
        }
        assert_eq!(minted(&state), 4);
    }

    #[test]
    fn a_replayed_reconnect_resumes_the_same_session() {
        let state = test_state(1000, 1000);
        let first = reserve_ok(&state, "joinOrCreate", "tank-3d", "{}");
        let body = format!("{{\"reconnectionToken\":\"{}\"}}", first.seat_token);
        let again = reserve_ok(&state, "reconnect", "tank-3d", &body);
        let third = reserve_ok(&state, "reconnect", "tank-3d", &body);
        assert_eq!(again.session_id, first.session_id, "sessionId must be stable");
        assert_eq!(third.session_id, first.session_id);
        assert_eq!(again.room_id, first.room_id);
        assert_eq!(again.seat_token, first.seat_token, "no token churn on resume");
        assert_eq!(minted(&state), 1, "reconnect mints nothing");
    }

    #[test]
    fn rejoin_resumes_from_the_reconnection_token_too() {
        let state = test_state(1000, 1000);
        let first = reserve_ok(&state, "create", "tank-3d", "{}");
        let body = format!("{{\"reconnectionToken\":\"{}\"}}", first.seat_token);
        let back = reserve_ok(&state, "rejoin", "tank-3d", &body);
        assert_eq!(back.session_id, first.session_id);
        assert_eq!(minted(&state), 1);
    }

    #[test]
    fn a_reconnect_cannot_jump_rooms_via_the_url() {
        // The seat's room comes from the signature, not from the path segment.
        let state = test_state(1000, 1000);
        let first = reserve_ok(&state, "create", "lobby_a", "{}");
        let body = format!("{{\"reconnectionToken\":\"{}\"}}", first.seat_token);
        let hijack = reserve_ok(&state, "reconnect", "lobby_b", &body);
        assert_eq!(hijack.room_id, "lobby_a", "the URL did not move the seat");
        assert_eq!(hijack.name, "lobby_a");
    }

    #[test]
    fn a_missing_or_malformed_token_is_a_bad_request() {
        let state = test_state(1000, 1000);
        for body in ["{}", "not json", "{\"reconnectionToken\":42}", ""] {
            let e = err(&state, ip(1), "reconnect", "tank-3d", body);
            assert_eq!(e.status, StatusCode::BAD_REQUEST, "body {body:?}");
        }
        assert_eq!(minted(&state), 0);
    }

    #[test]
    fn an_expired_or_tampered_token_is_refused() {
        let state = test_state(1000, 1000);
        let seat = reserve_ok(&state, "joinOrCreate", "tank-3d", "{}");
        let (payload, mac) = seat.seat_token.split_once('.').unwrap();
        let mut flipped: Vec<char> = mac.chars().collect();
        flipped[0] = if flipped[0] == 'A' { 'B' } else { 'A' };
        let tampered = format!("{payload}.{}", flipped.iter().collect::<String>());
        let expired = {
            let mut r = SeatRegistry::new(DEV_SEAT_SECRET);
            r.mint("tank-3d", 0).unwrap().token // ttl 0 => already expired
        };
        for token in [tampered, expired, format!("{payload}AAAA.{mac}"), "a.b".to_string()] {
            let e = err(
                &state,
                ip(1),
                "reconnect",
                "tank-3d",
                &format!("{{\"reconnectionToken\":\"{token}\"}}"),
            );
            assert_eq!(e.status, StatusCode::UNAUTHORIZED, "token {token}");
        }
        assert_eq!(minted(&state), 1, "refusals mint nothing");
    }

    #[test]
    fn an_unknown_method_is_404_and_a_bad_room_name_is_400() {
        let state = test_state(1000, 1000);
        assert_eq!(
            err(&state, ip(1), "spectateSoon", "tank-3d", "{}").status,
            StatusCode::NOT_FOUND
        );
        for room in ["", "../etc", "room name", "a\"b", "a/b", &"x".repeat(65)] {
            assert_eq!(
                err(&state, ip(1), "joinOrCreate", room, "{}").status,
                StatusCode::BAD_REQUEST,
                "room {room:?}"
            );
        }
        assert_eq!(minted(&state), 0);
    }

    #[test]
    fn a_hostile_room_name_cannot_break_the_json_envelope() {
        // Quotes and slashes in a path segment never reach the envelope: the
        // name is rejected before a seat is signed, and the refusal body is
        // itself parseable JSON.
        let state = test_state(1000, 1000);
        for room in ["evil\"name", "evil/name", "evil\nname", "evil\\name"] {
            let resp = err(&state, ip(1), "joinOrCreate", room, "{}").into_response();
            assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
            assert_eq!(
                resp.headers()
                    .get(axum::http::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .unwrap(),
                "application/json"
            );
        }
    }

    // ---- rate limiting: charged before the crypto ---------------------------

    #[test]
    fn the_n_plus_oneth_request_from_one_ip_is_denied_but_another_ip_passes() {
        let state = test_state(1, 3); // 1 token/s, burst of 3
        for i in 0..3 {
            assert!(
                reserve(&state, ip(1), "joinOrCreate", "tank-3d", "{}").is_ok(),
                "request {i} is inside the burst"
            );
        }
        let e = err(&state, ip(1), "joinOrCreate", "tank-3d", "{}");
        assert_eq!(e.status, StatusCode::TOO_MANY_REQUESTS);
        assert!(e.retry_after.unwrap() >= 1, "the denial says when to return");
        // A different source is untouched by that bucket.
        assert!(reserve(&state, ip(2), "joinOrCreate", "tank-3d", "{}").is_ok());
    }

    #[test]
    fn a_denied_request_costs_no_hmac_and_no_map_entry() {
        // The anti-spoofing/anti-cost property, stated as an assertion: after
        // the bucket says no, the registry must not have done any work.
        let state = test_state(1, 1);
        assert!(reserve(&state, ip(1), "joinOrCreate", "tank-3d", "{}").is_ok());
        let before = minted(&state);
        let denied = state.limiter.lock().unwrap().counts().1;
        for _ in 0..200 {
            assert!(reserve(&state, ip(1), "joinOrCreate", "tank-3d", "{}").is_err());
        }
        assert_eq!(minted(&state), before, "nothing was minted while denied");
        assert_eq!(state.limiter.lock().unwrap().counts().1, denied + 200);
    }

    #[test]
    fn a_denied_reconnect_is_refused_by_the_bucket_not_by_the_token() {
        // Cost ordering holds on the resume path too: an over-budget client does
        // not get its token checked at all.
        let state = test_state(1, 1);
        let seat = reserve_ok(&state, "joinOrCreate", "tank-3d", "{}");
        let body = format!("{{\"reconnectionToken\":\"{}\"}}", seat.seat_token);
        let e = err(&state, ip(1), "reconnect", "tank-3d", &body);
        assert_eq!(e.status, StatusCode::TOO_MANY_REQUESTS);
        // The same token from a fresh source still resumes.
        let back = ok(&state, ip(9), "reconnect", "tank-3d", &body);
        assert_eq!(back.session_id, seat.session_id);
    }

    #[test]
    fn the_retry_after_header_reaches_the_client() {
        let resp = ApiError::rate_limited(7).into_response();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            resp.headers()
                .get(axum::http::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok()),
            Some("7")
        );
    }

    // ---- HTTP layer ---------------------------------------------------------

    struct TestServer {
        addr: SocketAddr,
        rt: Option<tokio::runtime::Runtime>,
    }

    impl TestServer {
        fn start(state: ControlState) -> TestServer {
            let routes = router(&state);
            Self::serve(state, routes)
        }

        /// Serve `routes` exactly the way `ControlPlane::spawn` does — bound to a
        /// real port, with the peer address made available — so a test can hand
        /// in a *merged* router instead of only this file's.
        fn serve(state: ControlState, routes: Router<ControlState>) -> TestServer {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .unwrap();
            let listener = rt
                .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
                .unwrap();
            let addr = listener.local_addr().unwrap();
            // `ConnectInfo` is what the bucket is charged from, so the test has
            // to serve exactly the way this plane is meant to be served.
            let app = routes
                .with_state(state)
                .into_make_service_with_connect_info::<SocketAddr>();
            rt.spawn(async move {
                let _ = axum::serve(listener, app).await;
            });
            TestServer { addr, rt: Some(rt) }
        }

        /// One blocking HTTP POST with `Connection: close`, read to EOF.
        fn post(&self, path: &str, body: &str) -> (u16, Vec<(String, String)>, String) {
            let mut stream = TcpStream::connect(self.addr).expect("connect");
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let request = format!(
                "POST {path} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                self.addr,
                body.len()
            );
            stream.write_all(request.as_bytes()).unwrap();
            stream.flush().unwrap();
            let mut raw = Vec::new();
            stream.read_to_end(&mut raw).unwrap();
            let text = String::from_utf8_lossy(&raw).into_owned();
            let (head, response_body) = text.split_once("\r\n\r\n").expect("response has a body");
            let status = head
                .lines()
                .next()
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|c| c.parse().ok())
                .expect("status line");
            let headers = head
                .lines()
                .skip(1)
                .filter_map(|l| {
                    l.split_once(':')
                        .map(|(k, v)| (k.trim().to_lowercase(), v.trim().to_string()))
                })
                .collect();
            (status, headers, response_body.to_string())
        }
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            if let Some(rt) = self.rt.take() {
                rt.shutdown_timeout(Duration::from_secs(1));
            }
        }
    }

    fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
        headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    #[test]
    fn both_paths_serve_a_seat_over_http() {
        for path in ["/matchmake", "/api/matchmake"] {
            let server = TestServer::start(test_state(1000, 1000));
            let (status, headers, body) = server.post(&format!("{path}/joinOrCreate/tank-3d"), "{}");
            assert_eq!(status, 200, "{path}: {body}");
            assert_eq!(header(&headers, "content-type"), Some("application/json"));
            let v: serde_json::Value = serde_json::from_str(&body).expect("seat is JSON");
            assert_eq!(v["roomId"].as_str().unwrap(), "tank-3d");
            assert!(!v["sessionId"].as_str().unwrap().is_empty());
            assert!(v["seatToken"].as_str().unwrap().contains('.'));
            assert!(v["reconnectionToken"].as_str().unwrap().starts_with("dGFuay0z"));
        }
    }

    #[test]
    fn http_reconnect_hands_back_the_same_session_id() {
        let server = TestServer::start(test_state(1000, 1000));
        let (_, _, body) = server.post("/matchmake/joinOrCreate/tank-3d", "{}");
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        let token = v["reconnectionToken"].as_str().unwrap().to_string();
        let first = v["sessionId"].as_str().unwrap().to_string();
        let (status, _, again) = server.post(
            "/matchmake/reconnect/tank-3d",
            &format!("{{\"reconnectionToken\":\"{token}\"}}"),
        );
        assert_eq!(status, 200, "reconnect: {again}");
        let v2: serde_json::Value = serde_json::from_str(&again).unwrap();
        assert_eq!(v2["sessionId"].as_str().unwrap(), first.as_str());
        assert_eq!(v2["seatToken"].as_str().unwrap(), token.as_str());
    }

    #[test]
    fn http_over_the_burst_is_a_429_with_retry_after() {
        let server = TestServer::start(test_state(1, 2));
        for _ in 0..2 {
            let (status, _, body) = server.post("/matchmake/joinOrCreate/tank-3d", "{}");
            assert_eq!(status, 200, "in-burst request: {body}");
        }
        let (status, headers, body) = server.post("/matchmake/joinOrCreate/tank-3d", "{}");
        assert_eq!(status, 429, "body: {body}");
        let retry = header(&headers, "retry-after").expect("Retry-After is sent");
        let secs: u32 = retry.parse().expect("delta seconds");
        assert!((1..=2).contains(&secs), "honest wait, got {secs}");
        let v: serde_json::Value = serde_json::from_str(&body).expect("error body is JSON");
        assert!(v["error"].as_str().unwrap().contains("rate limit"));
    }

    #[test]
    fn http_refuses_a_hostile_room_name_and_the_body_stays_json() {
        let server = TestServer::start(test_state(1000, 1000));
        // `evil"`, `evil/`, `evil\n` — percent-encoded so the router still sees
        // exactly two path segments.
        for room in ["evil%22name", "evil%2Fname", "evil%0Aname"] {
            let (status, headers, body) = server.post(&format!("/matchmake/joinOrCreate/{room}"), "{}");
            assert_eq!(status, 400, "{room}: {body}");
            assert_eq!(header(&headers, "content-type"), Some("application/json"));
            serde_json::from_str::<serde_json::Value>(&body)
                .unwrap_or_else(|e| panic!("{room} body not JSON: {e}: {body:?}"));
        }
    }

    #[test]
    fn http_rejects_a_body_over_the_limit() {
        let server = TestServer::start(test_state(1000, 1000));
        let big = "x".repeat(MAX_BODY_BYTES + 1024);
        let (status, _, _) = server.post(
            "/matchmake/reconnect/tank-3d",
            &format!("{{\"reconnectionToken\":\"{big}\"}}"),
        );
        assert_eq!(status, 413, "the ceiling is enforced before parsing");
    }

    #[test]
    fn the_body_limit_and_the_seat_survive_being_merged_into_the_plane() {
        // The plane composes these routes with the other groups
        // (`ControlPlane::build_router` merges them), so the ceiling must travel
        // with *this* Router's layer rather than only guarding the standalone
        // one the test above serves.
        let state = test_state(1000, 1000);
        let neighbour = Router::new().route("/ping", post(|| async { "pong" }));
        let routes = router(&state).merge(neighbour);
        let server = TestServer::serve(state, routes);

        let (status, _, body) = server.post("/ping", "");
        assert_eq!((status, body.as_str()), (200, "pong"), "the neighbour still routes");

        let (status, _, body) = server.post("/api/matchmake/joinOrCreate/tank-3d", "{}");
        assert_eq!(status, 200, "merged seat: {body}");
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["roomId"].as_str().unwrap(), "tank-3d");

        let big = "x".repeat(MAX_BODY_BYTES + 1024);
        let (status, _, _) = server.post(
            "/api/matchmake/reconnect/tank-3d",
            &format!("{{\"reconnectionToken\":\"{big}\"}}"),
        );
        assert_eq!(status, 413, "the layer is not flattened away by the merge");
    }

    #[test]
    fn http_unknown_method_is_404_and_a_control_char_does_not_break_the_body() {
        let server = TestServer::start(test_state(1000, 1000));
        let (status, _, body) = server.post("/matchmake/spectate/tank-3d", "{}");
        assert_eq!(status, 404, "{body}");
        let (status, _, body) = server.post("/matchmake/spec%01tate/tank-3d", "{}");
        assert_eq!(status, 404);
        serde_json::from_str::<serde_json::Value>(&body)
            .unwrap_or_else(|e| panic!("error body must stay JSON, got {body:?}: {e}"));
    }

    // The seat minted here is meant to authenticate the raw-UDP `JOIN` on the
    // sim plane (`src/threads/seats.rs`, engine-server). That cross-plane
    // round-trip test cannot live here without a dependency on engine-server,
    // so it is tracked as an integration TODO; the byte format itself stays
    // pinned by the fixed vector in `seats.rs`.

    // ---- kept from the draft: escaping + parsing primitives ------------------

    #[test]
    fn room_names_are_restricted() {
        assert!(valid_room_name("tank-3d"));
        assert!(valid_room_name("arena_final"));
        assert!(!valid_room_name(""));
        assert!(!valid_room_name("../etc"));
        assert!(!valid_room_name("room name"));
        assert!(!valid_room_name(&"x".repeat(65)));
    }

    #[test]
    fn escaping_survives_a_hostile_string() {
        // `valid_room_name` already rejects the obvious ones, but the escaper
        // also runs over tokens and ICE blobs, so it holds on its own.
        assert_eq!(json_escape("a\"b"), "a\\\"b");
        assert_eq!(json_escape("a\\b"), "a\\\\b");
        assert_eq!(json_escape("a\u{1}b"), "a\\u0001b");
    }

    #[test]
    fn seat_json_is_parseable_and_nests_the_ice_blob_as_text() {
        let resp = SeatResponse {
            name: "tank-3d".into(),
            session_id: "s1".into(),
            room_id: "tank-3d".into(),
            process_id: "openoxide_1".into(),
            reconnection_token: "tok".into(),
            seat_token: "tok".into(),
            game_port: 7777,
            control_port: 8090,
            ice_servers: "[{\"urls\":[\"stun:stun.l.google.com:19302\"]}]".into(),
            expires_at: 1_800_000_000,
        };
        let outer: serde_json::Value = serde_json::from_str(&resp.to_json()).unwrap();
        assert_eq!(outer["sessionId"].as_str().unwrap(), "s1");
        assert_eq!(outer["gamePort"].as_u64().unwrap(), 7777);
        // The ICE blob arrives as a string, so the client parses it a second
        // time — the same contract the in-transport matchmake had.
        let ice = outer["iceServers"].as_str().unwrap();
        let inner: serde_json::Value = serde_json::from_str(ice).unwrap();
        assert_eq!(inner[0]["urls"][0].as_str().unwrap(), "stun:stun.l.google.com:19302");
    }

    #[test]
    fn a_quote_in_a_seat_field_cannot_break_the_envelope() {
        let resp = SeatResponse {
            name: "t\"ank".into(),
            session_id: "s1".into(),
            room_id: "r".into(),
            process_id: "p".into(),
            reconnection_token: "a\\b".into(),
            seat_token: "t".into(),
            game_port: 1,
            control_port: 2,
            ice_servers: "[]".into(),
            expires_at: 3,
        };
        let outer: serde_json::Value = serde_json::from_str(&resp.to_json()).unwrap();
        assert_eq!(outer["name"].as_str().unwrap(), "t\"ank");
        assert_eq!(outer["reconnectionToken"].as_str().unwrap(), "a\\b");
    }

    #[test]
    fn json_field_reads_flat_bodies_and_ignores_noise() {
        assert_eq!(
            json_string_field("{\"reconnectionToken\":\"abc\"}", "reconnectionToken").as_deref(),
            Some("abc")
        );
        assert!(json_string_field("not json", "reconnectionToken").is_none());
        assert!(json_string_field("{}", "reconnectionToken").is_none());
        assert!(json_string_field("{\"reconnectionToken\":42}", "reconnectionToken").is_none());
    }

    #[test]
    fn printable_keeps_error_text_json_safe() {
        assert_eq!(printable("join\u{1}OrCreate"), "joinOrCreate");
        assert_eq!(printable("a\"b\\c"), "a\"b\\c");
        assert_eq!(printable(&"x".repeat(200)).len(), 64);
    }
}
