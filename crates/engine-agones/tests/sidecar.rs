//! End-to-end tests against a fake Agones sidecar: a `std::net::TcpListener` on
//! 127.0.0.1:0 that records the request line of every connection and answers with
//! a scripted, hand-composed HTTP reply.
//!
//! Everything is on 20–40 ms intervals so the whole file runs well under a second
//! of wall time; nothing here sleeps on the real 2 s production interval.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use engine_agones::{
    AgonesClient, AgonesController, AgonesSnapshot, Healthbeat, Lifecycle, LifecycleEvent, Reply,
    SidecarConfig, SidecarError, SidecarErrorKind, ServerState,
};

// ---------------------------------------------------------------------------
// Fake sidecar
// ---------------------------------------------------------------------------

/// One request the fake accepted, as the client wrote it.
#[derive(Debug, Clone)]
struct Request {
    method: String,
    path: String,
    version: String,
    head: String,
}

/// Compose a reply with `Content-Length`.
fn ok(body: &[u8]) -> Vec<u8> {
    framed(200, "OK", body)
}

fn framed(status: u16, reason: &str, body: &[u8]) -> Vec<u8> {
    let mut out = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    out.extend_from_slice(body);
    out
}

/// A reply with no length at all: only a `Connection: close` peer can express it,
/// and our client must still parse it.
fn close_delimited(body: &[u8]) -> Vec<u8> {
    let mut out = b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_vec();
    out.extend_from_slice(body);
    out
}

/// A `Transfer-Encoding: chunked` reply, split into small chunks.
fn chunked(body: &[u8]) -> Vec<u8> {
    let mut out =
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n".to_vec();
    for piece in body.chunks(3) {
        out.extend_from_slice(format!("{:x}\r\n", piece.len()).as_bytes());
        out.extend_from_slice(piece);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(b"0\r\n\r\n");
    out
}

/// Headers claim a body that never arrives.
fn truncated_body() -> Vec<u8> {
    b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n{\"st".to_vec()
}

/// Not HTTP at all.
fn garbage() -> Vec<u8> {
    b"this is not a status line\r\n\r\n".to_vec()
}

/// A valid frame whose JSON body is not UTF-8.
fn non_utf8_body() -> Vec<u8> {
    let mut body = vec![0xff, 0xfe, 0x00, b'{'];
    body.extend_from_slice(b"\"state\":\"Ready\"}");
    framed(200, "OK", &body)
}

/// Closes the connection without writing a byte.
fn hang_up() -> Vec<u8> {
    Vec::new()
}

struct FakeInner {
    requests: Vec<Request>,
    replies: VecDeque<Vec<u8>>,
    fallback: Vec<u8>,
    /// Per-path scripted replies, asked before the ordered queue. This is how a
    /// test can make `/ready` fail while `/health` succeeds — the two are
    /// interleaved in time, so an ordered script cannot express it.
    routes: Vec<(String, Vec<u8>)>,
    hang: bool,
}

/// The fake sidecar itself. One connection per request, like the real thing.
struct FakeSidecar {
    addr: SocketAddr,
    inner: Arc<Mutex<FakeInner>>,
    stop: Arc<AtomicBool>,
    serve: Option<JoinHandle<()>>,
}

impl FakeSidecar {
    /// Answer every request with `fallback`, except the scripted ones in order.
    fn scripted(script: Vec<Vec<u8>>, fallback: Vec<u8>) -> FakeSidecar {
        FakeSidecar::with_routes(script, fallback, Vec::new())
    }

    /// Like [`FakeSidecar::scripted`], but the named paths always get their own
    /// reply, no matter when they arrive.
    fn routing(routes: Vec<(&str, Vec<u8>)>, fallback: Vec<u8>) -> FakeSidecar {
        FakeSidecar::with_routes(
            Vec::new(),
            fallback,
            routes.into_iter().map(|(path, reply)| (path.to_string(), reply)).collect(),
        )
    }

    fn with_routes(
        script: Vec<Vec<u8>>,
        fallback: Vec<u8>,
        routes: Vec<(String, Vec<u8>)>,
    ) -> FakeSidecar {
        let listener = TcpListener::bind("127.0.0.1:0").expect("fake sidecar binds");
        let addr = listener.local_addr().expect("fake sidecar has an address");
        let inner = Arc::new(Mutex::new(FakeInner {
            requests: Vec::new(),
            replies: VecDeque::from(script),
            fallback,
            routes,
            hang: false,
        }));
        let stop = Arc::new(AtomicBool::new(false));

        let (thread_inner, thread_stop) = (Arc::clone(&inner), Arc::clone(&stop));
        listener.set_nonblocking(true).expect("nonblocking accept");
        let serve = std::thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let request = read_request(&mut stream);
                        let (reply, hang) = {
                            let mut state = thread_inner.lock().unwrap();
                            if let Some(request) = &request {
                                state.requests.push(request.clone());
                            }
                            let routed = request
                                .as_ref()
                                .and_then(|r| state.routes.iter().find(|(p, _)| *p == r.path))
                                .map(|(_, reply)| reply.clone());
                            let reply = routed.unwrap_or_else(|| {
                                state
                                    .replies
                                    .pop_front()
                                    .unwrap_or_else(|| state.fallback.clone())
                            });
                            (reply, state.hang)
                        };
                        if request.is_none() {
                            continue;
                        }
                        if hang {
                            // A peer that accepts but never answers: the client
                            // must hit its own deadline, not wait for us.
                            let until = Instant::now() + Duration::from_secs(2);
                            while Instant::now() < until && !thread_stop.load(Ordering::Relaxed) {
                                std::thread::sleep(Duration::from_millis(5));
                            }
                            continue;
                        }
                        let _ = stream.write_all(&reply);
                        let _ = stream.flush();
                        // Dropping `stream` closes it, which is what a
                        // close-delimited body needs.
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    Err(_) => break,
                }
            }
        });

        FakeSidecar { addr, inner, stop, serve: Some(serve) }
    }

    fn always(reply: Vec<u8>) -> FakeSidecar {
        FakeSidecar::scripted(Vec::new(), reply)
    }

    /// Accept connections but never answer them.
    fn silent() -> FakeSidecar {
        let fake = FakeSidecar::always(ok(b"{}"));
        fake.inner.lock().unwrap().hang = true;
        fake
    }

    fn client(&self, timeout: Duration) -> AgonesClient {
        AgonesClient::new(self.config(timeout))
    }

    fn config(&self, timeout: Duration) -> SidecarConfig {
        SidecarConfig { addr: self.addr.to_string(), timeout }
    }

    fn requests(&self) -> Vec<Request> {
        self.inner.lock().unwrap().requests.clone()
    }

    fn paths(&self) -> Vec<String> {
        self.requests().into_iter().map(|r| r.path).collect()
    }

    fn count(&self, path: &str) -> usize {
        self.requests().into_iter().filter(|r| r.path == path).count()
    }

    fn request_for(&self, path: &str) -> Option<Request> {
        self.requests().into_iter().find(|r| r.path == path)
    }

    fn wait_for_requests(&self, path: &str, at_least: usize, budget: Duration) -> usize {
        let until = Instant::now() + budget;
        loop {
            let seen = self.count(path);
            if seen >= at_least || Instant::now() >= until {
                return seen;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

impl Drop for FakeSidecar {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.serve.take() {
            let _ = handle.join();
        }
    }
}

/// Read the request head, stopping at the blank line or a short timeout.
fn read_request(stream: &mut TcpStream) -> Option<Request> {
    stream.set_read_timeout(Some(Duration::from_millis(200))).ok();
    let mut buf = [0u8; 1024];
    let mut head = Vec::new();
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                head.extend_from_slice(&buf[..n]);
                if head.windows(4).any(|w| w == b"\r\n\r\n") || head.len() > 8192 {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    if head.is_empty() {
        return None;
    }
    let text = String::from_utf8_lossy(&head).into_owned();
    let first = text.lines().next().unwrap_or("").to_string();
    let mut fields = first.split_whitespace();
    Some(Request {
        method: fields.next().unwrap_or("").to_string(),
        path: fields.next().unwrap_or("").to_string(),
        version: fields.next().unwrap_or("").to_string(),
        head: text,
    })
}

/// An address that refuses connections: bind, learn the port, drop the listener.
fn closed_port() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind to find a free port");
    let addr = listener.local_addr().expect("local addr");
    drop(listener);
    addr.to_string()
}

// ---------------------------------------------------------------------------
// Transitions
// ---------------------------------------------------------------------------

#[test]
fn ready_happy_path_posts_the_agones_request_line() {
    let fake = FakeSidecar::always(ok(b"{}"));
    let client = fake.client(Duration::from_millis(500));

    client.mark_ready().expect("/ready should be accepted");

    let request = fake.request_for("/ready").expect("the sidecar saw /ready");
    assert_eq!(request.method, "POST");
    assert_eq!(request.version, "HTTP/1.1");
    assert!(request.head.contains("Connection: close"), "{}", request.head);
    assert!(request.head.contains("Content-Length: 0"), "{}", request.head);
    assert_eq!(fake.paths(), vec!["/ready".to_string()]);
}

#[test]
fn a_rejected_ready_is_a_rejection_not_a_panic() {
    let fake = FakeSidecar::always(framed(
        400,
        "Bad Request",
        b"GameServer is in state Shutdown, cannot go Ready",
    ));
    let client = fake.client(Duration::from_millis(500));

    let error = client.mark_ready().expect_err("a 400 must not look like success");

    match &error {
        SidecarError::Rejected { status, body, .. } => {
            assert_eq!(*status, 400);
            assert!(body.contains("cannot go Ready"), "{body}");
        }
        other => panic!("expected a rejection, got {other:?}"),
    }
    // The two failure kinds stay distinguishable.
    assert_eq!(error.kind(), SidecarErrorKind::Rejected);
    assert!(!error.kind().retryable());
}

#[test]
fn a_refused_sidecar_is_unreachable_not_rejected() {
    let client = AgonesClient::new(SidecarConfig {
        addr: closed_port(),
        timeout: Duration::from_millis(200),
    });
    let error = client.mark_ready().expect_err("nothing is listening");
    assert_eq!(error.kind(), SidecarErrorKind::Unreachable);
    assert!(error.message().contains("cannot reach sidecar"), "{}", error.message());
}

#[test]
fn allocated_and_shutdown_use_their_own_paths() {
    let fake = FakeSidecar::always(ok(b"{}"));
    let client = fake.client(Duration::from_millis(500));

    client.mark_allocated().expect("POST /allocated");
    client.shutdown().expect("POST /shutdown");

    assert_eq!(fake.paths(), vec!["/allocated".to_string(), "/shutdown".to_string()]);
    assert_eq!(fake.request_for("/allocated").unwrap().method, "POST");
    assert_eq!(fake.request_for("/shutdown").unwrap().method, "POST");
}

#[test]
fn a_success_body_is_returned_to_the_caller() {
    let fake = FakeSidecar::always(ok(b"{\"state\":\"Ready\"}"));
    let reply: Reply = fake
        .client(Duration::from_millis(500))
        .call("GET", "/state")
        .expect("GET /state");
    assert_eq!(reply.status, 200);
    assert_eq!(reply.reason, "OK");
    assert!(reply.is_success());
    assert_eq!(reply.body_lossy(), "{\"state\":\"Ready\"}");
}

// ---------------------------------------------------------------------------
// GET /state parsing
// ---------------------------------------------------------------------------

#[test]
fn state_json_parses_with_metadata() {
    let fake = FakeSidecar::always(ok(
        b"{\"state\":\"Allocated\",\"metadata\":{\"name\":\"pod-7\",\"ip\":\"10.0.0.9\"}}",
    ));
    let snapshot = fake.client(Duration::from_millis(500)).get_state().expect("parses");

    assert_eq!(snapshot.state, ServerState::Allocated);
    assert_eq!(snapshot.state_raw, "Allocated");
    assert_eq!(snapshot.metadata["name"], "pod-7");
    assert_eq!(snapshot.metadata["ip"], "10.0.0.9");
    assert_eq!(fake.request_for("/state").unwrap().method, "GET");
}

#[test]
fn an_unknown_state_name_is_unknown_not_an_error() {
    let fake = FakeSidecar::always(ok(b"{\"state\":\"Teleported\"}"));
    let snapshot = fake.client(Duration::from_millis(500)).get_state().expect("parses");
    assert_eq!(snapshot.state, ServerState::Unknown);
    assert_eq!(snapshot.display(), "Unknown(Teleported)");
}

#[test]
fn state_without_a_json_body_is_malformed() {
    let fake = FakeSidecar::always(ok(b"not json at all"));
    let error = fake
        .client(Duration::from_millis(500))
        .get_state()
        .expect_err("unparseable JSON must be an error");
    assert_eq!(error.kind(), SidecarErrorKind::Malformed);
}

#[test]
fn state_json_without_a_state_field_is_malformed() {
    let fake = FakeSidecar::always(ok(b"{\"metadata\":{\"name\":\"pod-7\"}}"));
    let error = fake
        .client(Duration::from_millis(500))
        .get_state()
        .expect_err("a payload with no phase is not a phase");
    assert_eq!(error.kind(), SidecarErrorKind::Malformed);
    assert!(error.message().contains("no lifecycle state"), "{}", error.message());
}

#[test]
fn state_json_without_metadata_still_parses() {
    let fake = FakeSidecar::always(ok(b"{\"state\":\"Ready\"}"));
    let snapshot = fake.client(Duration::from_millis(500)).get_state().expect("parses");
    assert_eq!(snapshot.state, ServerState::Ready);
    assert_eq!(snapshot.metadata, serde_json::json!({}));
}

#[test]
fn a_non_utf8_state_body_never_panics() {
    let fake = FakeSidecar::always(non_utf8_body());
    let error = fake
        .client(Duration::from_millis(500))
        .get_state()
        .expect_err("invalid UTF-8 is not a state");
    assert_eq!(error.kind(), SidecarErrorKind::Malformed, "{error:?}");
}

#[test]
fn a_truncated_reply_is_malformed_never_a_panic() {
    let fake = FakeSidecar::always(truncated_body());
    let error = fake
        .client(Duration::from_millis(500))
        .get_state()
        .expect_err("100 promised bytes, 4 delivered");
    assert_eq!(error.kind(), SidecarErrorKind::Malformed);
    assert!(error.message().contains("truncated"), "{}", error.message());
}

#[test]
fn a_reply_that_is_not_http_is_malformed() {
    let fake = FakeSidecar::always(garbage());
    let error = fake.client(Duration::from_millis(500)).get_state().unwrap_err();
    assert_eq!(error.kind(), SidecarErrorKind::Malformed);
}

#[test]
fn a_server_error_on_a_transition_keeps_its_status_code() {
    let fake = FakeSidecar::always(framed(500, "Internal Server Error", b"agent is unhappy"));
    let error = fake.client(Duration::from_millis(500)).send_health_ping().unwrap_err();

    match error {
        SidecarError::Rejected { status, body, .. } => {
            assert_eq!(status, 500, "the code is the actionable part");
            assert_eq!(body, "agent is unhappy");
        }
        other => panic!("a 5xx answer is a rejection, got {other:?}"),
    }
    assert_eq!(fake.count("/health"), 1, "and one connection per attempt");
}

#[test]
fn a_close_delimited_reply_with_no_length_still_parses() {
    let fake = FakeSidecar::always(close_delimited(b"{\"state\":\"Ready\"}"));
    let snapshot = fake
        .client(Duration::from_millis(500))
        .get_state()
        .expect("the body ends at the close");
    assert_eq!(snapshot.state, ServerState::Ready);
}

#[test]
fn a_chunked_reply_parses() {
    let fake = FakeSidecar::always(chunked(b"{\"state\":\"NotReady\",\"metadata\":{}}"));
    let snapshot = fake.client(Duration::from_millis(500)).get_state().expect("chunks decode");
    assert_eq!(snapshot.state, ServerState::NotReady);
    assert_eq!(snapshot.metadata, serde_json::json!({}));
}

#[test]
fn a_sidecar_that_accepts_but_never_answers_times_out() {
    let fake = FakeSidecar::silent();
    let started = Instant::now();
    let error = fake
        .client(Duration::from_millis(120))
        .send_health_ping()
        .expect_err("a silent peer must not hang us");
    let elapsed = started.elapsed();

    assert_eq!(error.kind(), SidecarErrorKind::Timeout, "{error:?}");
    assert!(elapsed < Duration::from_millis(900), "waited {elapsed:?}");
    assert!(fake.count("/health") >= 1, "the request was at least written");
}

#[test]
fn a_peer_that_hangs_up_mid_reply_is_reported_not_fatal() {
    let fake = FakeSidecar::always(hang_up());
    let error = fake.client(Duration::from_millis(300)).mark_ready().unwrap_err();
    assert_eq!(error.kind(), SidecarErrorKind::Malformed, "{error:?}");
}

// ---------------------------------------------------------------------------
// The health beat
// ---------------------------------------------------------------------------

#[test]
fn the_ping_loop_survives_refused_connections_and_reports_the_streak() {
    let stop = Arc::new(AtomicBool::new(false));
    let addr = closed_port();
    let handle = {
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let client = AgonesClient::new(SidecarConfig { addr, timeout: Duration::from_millis(50) });
            let mut beat = Healthbeat::new(client, Duration::from_millis(40));
            beat.run_until_stop(&stop, |_outcome| {});
            beat.stats()
        })
    };

    std::thread::sleep(Duration::from_millis(220));
    stop.store(true, Ordering::Relaxed);
    let stats = handle.join().expect("the beat loop ended cleanly");

    assert!(stats.pings >= 3, "only {} pings in 220 ms at a 40 ms interval", stats.pings);
    assert_eq!(stats.pings, stats.failures, "every ping was refused");
    assert_eq!(stats.successes, 0);
    assert_eq!(stats.consecutive_failures, stats.pings as u32);
    assert_eq!(stats.unreachable_failures, stats.pings as u32);
    assert!(stats.unhealthy_declared, "three misses must be visible in the stats");
}

#[test]
fn the_ping_loop_keeps_beating_a_live_sidecar() {
    let fake = FakeSidecar::always(ok(b"{}"));
    let stop = Arc::new(AtomicBool::new(false));
    let config = fake.config(Duration::from_millis(200));

    let handle = {
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut beat = Healthbeat::new(AgonesClient::new(config), Duration::from_millis(30));
            beat.run_until_stop(&stop, |_outcome| {});
            beat.stats()
        })
    };

    let seen = fake.wait_for_requests("/health", 4, Duration::from_millis(500));
    stop.store(true, Ordering::Relaxed);
    let stats = handle.join().expect("the beat loop ended cleanly");

    assert!(seen >= 4, "expected at least 4 heartbeats, saw {seen}");
    assert_eq!(stats.failures, 0, "a live sidecar must not produce failures");
    assert_eq!(stats.consecutive_failures, 0);
    assert!(stats.pings as usize >= seen);
}

#[test]
fn backoff_grows_with_the_streak_and_resets_on_recovery() {
    let fake = FakeSidecar::scripted(
        vec![
            framed(503, "Service Unavailable", b"busy"),
            framed(503, "Service Unavailable", b"busy"),
            framed(503, "Service Unavailable", b"busy"),
            framed(503, "Service Unavailable", b"busy"),
        ],
        ok(b"{}"),
    );
    let interval = Duration::from_millis(40);
    let mut beat = Healthbeat::new(fake.client(Duration::from_millis(200)), interval);

    let first = beat.ping(Instant::now());
    assert!(!first.ok);
    assert_eq!(first.consecutive_failures, 1);
    assert_eq!(beat.current_delay(), interval, "one miss is not worth backing off");

    let second = beat.ping(Instant::now());
    assert_eq!(second.consecutive_failures, 2);
    assert_eq!(beat.current_delay(), interval * 2);

    let third = beat.ping(Instant::now());
    assert_eq!(third.consecutive_failures, 3);
    assert_eq!(beat.current_delay(), interval * 4);
    assert!(third.just_missed_the_window, "the third miss is the loud one");
    assert_eq!(third.error.unwrap().kind(), SidecarErrorKind::Rejected);

    let fourth = beat.ping(Instant::now());
    assert_eq!(fourth.consecutive_failures, 4);
    assert_eq!(beat.current_delay(), interval * 8, "and the ceiling is eight intervals");
    assert!(!fourth.just_missed_the_window, "the window is crossed once per streak, not every tick");

    let recovered = beat.ping(Instant::now());
    assert!(recovered.ok, "the scripted 503s are exhausted");
    assert_eq!(recovered.consecutive_failures, 0);
    assert_eq!(beat.current_delay(), interval, "healthy means the plain interval again");

    let stats = beat.stats();
    assert_eq!(stats.rejected_failures, 4);
    assert_eq!(stats.successes, 1);
    assert_eq!(stats.max_consecutive_failures, 4);
    assert!(stats.unhealthy_declared);
}

#[test]
fn the_backoff_ceiling_keeps_recovery_fast() {
    // A real refused connection returns in microseconds, so a long streak is
    // cheap to build: the delay it produces is what must stay bounded.
    let mut beat = Healthbeat::new(
        AgonesClient::new(SidecarConfig { addr: closed_port(), timeout: Duration::from_millis(30) }),
        Duration::from_millis(50),
    );
    for _ in 0..9 {
        beat.ping(Instant::now());
    }
    assert_eq!(beat.stats().consecutive_failures, 9);
    assert_eq!(beat.current_delay(), Duration::from_millis(400), "8 × the interval, never more");
    assert!(beat.current_delay() <= beat.max_backoff());

    // The ceiling is configurable for a pod with a slow agent.
    let tuned = Healthbeat::new(
        AgonesClient::new(SidecarConfig { addr: closed_port(), timeout: Duration::from_millis(20) }),
        Duration::from_millis(2000),
    )
    .with_max_backoff(Duration::from_secs(3));
    assert_eq!(tuned.max_backoff(), Duration::from_secs(3));
    assert_eq!(tuned.interval(), Duration::from_millis(2000));
}

// ---------------------------------------------------------------------------
// Lifecycle and the /status view
// ---------------------------------------------------------------------------

#[test]
fn lifecycle_records_ready_then_pings_and_shuts_down_once() {
    let fake = FakeSidecar::always(ok(b"{}"));
    let lifecycle = Lifecycle::new(fake.config(Duration::from_millis(300)), Duration::from_millis(40));

    lifecycle.mark_ready().expect("ready");
    assert_eq!(fake.request_for("/ready").unwrap().method, "POST");

    let outcome = lifecycle.ping_once();
    assert!(outcome.ok, "{outcome:?}");

    let snap = lifecycle.snapshot();
    assert!(snap.enabled);
    assert_eq!(snap.state, ServerState::Ready);
    assert!(snap.ready);
    assert_eq!(snap.last_transition, "ready");
    assert_eq!(snap.pings.pings, 1);
    assert_eq!(snap.pings.successes, 1);
    assert!(snap.last_error.is_none());

    assert!(lifecycle.shutdown().expect("first shutdown sends it"));
    assert!(!lifecycle.shutdown().expect("second shutdown is a no-op"), "idempotent");
    assert_eq!(fake.count("/shutdown"), 1, "the sidecar is told exactly once");
    assert!(lifecycle.is_stopped(), "and the beat is done");

    let snap = lifecycle.snapshot();
    assert_eq!(snap.state, ServerState::Shutdown);
    assert!(snap.shutdown_sent);
    assert!(snap.stopped);
    assert_eq!(snap.last_transition, "shutdown");
}

#[test]
fn lifecycle_shutdown_is_retried_after_an_unreachable_sidecar() {
    let lifecycle = Lifecycle::new(
        SidecarConfig { addr: closed_port(), timeout: Duration::from_millis(50) },
        Duration::from_millis(40),
    );
    let error = lifecycle.shutdown().expect_err("nothing is listening");
    assert_eq!(error.kind(), SidecarErrorKind::Unreachable);

    // The claim was released, so a later attempt can still reach the sidecar.
    let snap: AgonesSnapshot = lifecycle.snapshot();
    assert!(!snap.shutdown_sent, "a failed shutdown must not be recorded as sent");
    assert_eq!(snap.last_error.map(|e| e.kind).as_deref(), Some("unreachable"));
    assert_eq!(snap.last_transition, "shutdown");
}

#[test]
fn lifecycle_keeps_a_rejection_in_the_snapshot() {
    let fake = FakeSidecar::always(framed(400, "Bad Request", b"GameServer is Shutdown"));
    let lifecycle = Lifecycle::new(fake.config(Duration::from_millis(300)), Duration::from_millis(40));

    let error = lifecycle.mark_ready().expect_err("rejected");
    assert_eq!(error.kind(), SidecarErrorKind::Rejected);

    let snap = lifecycle.snapshot();
    assert!(!snap.ready, "a rejected /ready never put us in the pool");
    assert_eq!(snap.state, ServerState::Unknown);
    let info = snap.last_error.expect("the rejection is reportable");
    assert_eq!(info.kind, "rejected");
    assert!(info.message.contains("GameServer is Shutdown"), "{}", info.message);
}

#[test]
fn lifecycle_state_refresh_overwrites_an_inferred_state() {
    let fake = FakeSidecar::scripted(
        vec![ok(b"{}"), ok(b"{\"state\":\"Allocated\",\"metadata\":{\"room\":\"dm\"}}")],
        ok(b"{}"),
    );
    let lifecycle = Lifecycle::new(fake.config(Duration::from_millis(300)), Duration::from_millis(40));
    lifecycle.mark_ready().expect("ready");
    let snapshot = lifecycle.refresh_state().expect("state");

    assert_eq!(snapshot.state, ServerState::Allocated);
    assert_eq!(snapshot.metadata["room"], "dm");
    assert_eq!(fake.paths(), vec!["/ready".to_string(), "/state".to_string()]);

    let snap = lifecycle.snapshot();
    assert_eq!(snap.state, ServerState::Allocated, "the view follows the sidecar, not our guess");
    assert_eq!(snap.state_raw, "Allocated");
    assert_eq!(snap.last_transition, "state");
    assert!(snap.last_error.is_none(), "a good read clears the old failure");
}

#[test]
fn a_failing_beat_loop_is_visible_in_the_status_snapshot() {
    let lifecycle = Lifecycle::new(
        SidecarConfig { addr: closed_port(), timeout: Duration::from_millis(30) },
        Duration::from_millis(40),
    );
    for _ in 0..3 {
        let outcome = lifecycle.ping_once();
        assert!(!outcome.ok);
    }
    let snap = lifecycle.snapshot();
    assert_eq!(snap.pings.consecutive_failures, 3);
    assert_eq!(snap.pings.unreachable_failures, 3);
    assert!(snap.pings.unhealthy_declared, "three misses is the Agones window");
    assert_eq!(snap.backoff_ms, 160, "the doubling is reportable");
    assert_eq!(snap.interval_ms, 40);
    assert!(snap.enabled);
    let json = serde_json::to_string(&snap).expect("the payload is serialisable");
    assert!(json.contains("\"unhealthyDeclared\":true"), "{json}");
    assert!(json.contains("\"consecutiveFailures\":3"), "{json}");
}

#[test]
fn the_global_snapshot_says_not_deployed_when_nothing_is_installed() {
    // `install` is process-wide, so this test only asserts the *type's* default;
    // the global is exercised by the control plane's own tests.
    let snap = AgonesSnapshot::disabled();
    assert!(!snap.enabled);
    assert_eq!(snap.state, ServerState::Unknown);
    assert!(snap.addr.is_empty());
    assert_eq!(snap.pings, engine_agones::PingStats::default());

    let lifecycle = Lifecycle::new(
        SidecarConfig { addr: closed_port(), timeout: Duration::from_millis(20) },
        Duration::from_millis(2000),
    );
    let json = serde_json::to_value(lifecycle.snapshot()).expect("a JSON object");
    assert_eq!(json["enabled"], serde_json::json!(true));
    assert_eq!(json["intervalMs"], serde_json::json!(2000));
    assert_eq!(json["consecutiveFailures"], serde_json::json!(0));
}

// ---------------------------------------------------------------------------
// The start-of-pod race: /ready retried from inside the beat loop
// ---------------------------------------------------------------------------

/// Run the beat loop on its own thread for `budget`, recording every
/// `ReadyRetried` event as the kind of error it carried — `None` means the retry
/// was accepted. The caller keeps the `Arc<Lifecycle>`, so it can still read the
/// snapshot and the request log afterwards.
fn ready_retries_seen(lifecycle: &Arc<Lifecycle>, budget: Duration) -> Vec<Option<String>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let runner = {
        let lifecycle = Arc::clone(lifecycle);
        std::thread::spawn(move || {
            lifecycle.run_until_stopped(|event| {
                if let LifecycleEvent::ReadyRetried(error) = event {
                    sink.lock().expect("the collector does not panic").push(
                        error.map(|e| e.kind().to_string()),
                    );
                }
            });
        })
    };

    std::thread::sleep(budget);
    lifecycle.stop();
    runner.join().expect("the beat loop ended cleanly");
    seen.lock().expect("the collector does not panic").clone()
}

#[test]
fn a_pod_that_was_never_ready_is_asked_again_once_the_sidecar_answers() {
    // The normal start-of-pod race: `/ready` has not been accepted (here it was
    // never even attempted), but the sidecar is up now. The first heartbeat that
    // gets a reply is the moment to ask again, because a pod that never became
    // Ready is never in the allocation pool.
    let fake = FakeSidecar::always(ok(b"{}"));
    let lifecycle = Lifecycle::new(
        fake.config(Duration::from_millis(200)),
        Duration::from_millis(30),
    );

    let retries = ready_retries_seen(&lifecycle, Duration::from_millis(200));

    assert_eq!(retries, vec![None], "exactly one retry, and it was accepted");
    assert!(
        lifecycle.snapshot().ready,
        "the loop settled the lifecycle without the caller's help"
    );
    assert_eq!(lifecycle.snapshot().state, ServerState::Ready);
    // The heartbeat came first: the retry is gated on proof that there is a
    // sidecar, never on a timer.
    let paths = fake.paths();
    assert_eq!(paths.first().map(String::as_str), Some("/health"), "{paths:?}");
    assert_eq!(fake.count("/ready"), 1, "and it is not asked again once accepted");
    assert!(fake.count("/health") >= 2, "the beat kept its own cadence: {paths:?}");
}

#[test]
fn a_rejected_ready_is_never_asked_again() {
    // The other half of the rule: a sidecar that *answered* `/ready` with a 400 is
    // saying this GameServer must not be Ready. Re-sending it every interval is
    // worse than logging it once, so the loop tries once and stops — while the
    // heartbeats carry on, because those are a different question.
    let fake = FakeSidecar::routing(
        vec![("/ready", framed(400, "Bad Request", b"GameServer is Shutdown"))],
        ok(b"{}"),
    );
    let lifecycle = Lifecycle::new(
        fake.config(Duration::from_millis(200)),
        Duration::from_millis(30),
    );

    let retries = ready_retries_seen(&lifecycle, Duration::from_millis(200));

    assert_eq!(retries, vec![Some("rejected".to_string())], "reported, then dropped");
    assert_eq!(fake.count("/ready"), 1, "a rejection stops the retries for good");
    assert!(fake.count("/health") >= 2, "the beat did not stop on a rejected /ready");
    let snap = lifecycle.snapshot();
    assert!(!snap.ready, "a refused pod is not in the pool");
    // A later healthy heartbeat clears `last_error` (health is genuinely fine), so
    // the durable signal here is that we stopped retrying, not the error field.
    assert!(lifecycle.ready_refused());
}


