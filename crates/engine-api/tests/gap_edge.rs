//! The edge surface no earlier gap suite drives end to end: the liveness probes,
//! the root redirect, the static/SDK file plane, the dashboard assets, the live
//! in-process registry behind `/api/dashboard/*` and its websocket, the seat
//! gate on `/ws/signal`, and both matchmake spellings with their full refusal
//! vocabulary.
//!
//! Same harness as `gap_console.rs`: Postgres 16 in a Testcontainer, the real
//! router on a real socket. Where the static plane is expected to miss (the
//! harness's statics root is the crate directory, which holds no Unity build,
//! no `cyg.js` and no `arena_map.json`), the assertions pin the real 404 — the
//! plain-text body the handlers write — rather than an imagined 200.

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures::{SinkExt, StreamExt};
use reqwest::redirect::Policy;
use tokio_tungstenite::tungstenite::Message as Raw;
use uuid::Uuid;

#[path = "harness/mod.rs"]
mod harness;

use harness::*;

/// The plain-JSON websocket the dashboard and signal planes speak: one object
/// per text frame, no `cid` envelope (that protocol is `/ws` proper, covered by
/// `gap_ws.rs`). Queue-free by design: these planes push, they do not reply.
struct PlainWs {
    name: String,
    ws: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
}

impl PlainWs {
    async fn connect(name: &str, url: String) -> PlainWs {
        let (ws, _) = tokio_tungstenite::connect_async(&url)
            .await
            .unwrap_or_else(|err| panic!("{name}: socket connects: {err}"));
        PlainWs {
            name: name.into(),
            ws,
        }
    }

    /// The next JSON frame, within 10 s; control frames are skipped.
    async fn next_json(&mut self, describe: &str) -> Value {
        loop {
            let msg = tokio::time::timeout(Duration::from_secs(10), self.ws.next())
                .await
                .unwrap_or_else(|_| panic!("{}: a frame within 10 s ({describe})", self.name));
            let Some(Ok(msg)) = msg else {
                panic!("{}: socket closed while waiting for {describe}", self.name);
            };
            match msg {
                Raw::Text(text) => {
                    return serde_json::from_str(text.as_str()).unwrap_or_else(|err| {
                        panic!("{}: frame is JSON: {err}: {text}", self.name)
                    });
                }
                Raw::Close(_) => panic!("{}: server closed before {describe}", self.name),
                // Ping/Pong are protocol noise here; the dashboard/signal planes
                // carry their contract in text frames.
                _ => {}
            }
        }
    }

    /// Ask for a clean close so the server task (and the harness's graceful
    /// shutdown) does not have to wait on an idle socket.
    async fn shutdown(mut self) {
        let _ = tokio::time::timeout(Duration::from_secs(2), self.ws.send(Raw::Close(None))).await;
    }
}

/// Status, content type and body of a plain-text response, asserted together.
async fn expect_text(resp: reqwest::Response, status: StatusCode, what: &str) -> (String, String) {
    let actual = resp.status();
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let body = resp.text().await.unwrap_or_default();
    assert_eq!(
        actual, status,
        "{what}: expected {status}, got {actual}: {body}"
    );
    (content_type, body)
}

/// A binary response body, with the exact status asserted.
async fn expect_bytes(resp: reqwest::Response, status: StatusCode, what: &str) -> Vec<u8> {
    let actual = resp.status();
    let body = resp.bytes().await.unwrap_or_default().to_vec();
    assert_eq!(actual, status, "{what}: expected {status}, got {actual}");
    body
}

/// The MIME the static plane's own table writes for a built dashboard asset.
/// Broad on the extension is fine: the suite reads the file it fetches back.
fn expected_mime(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or_default() {
        "js" | "mjs" => "text/javascript",
        "css" => "text/css",
        "png" => "image/png",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after 1970")
        .as_secs()
}

// ---------------------------------------------------------------------------
// stages
// ---------------------------------------------------------------------------

/// `GET /health` and `GET /auto/health` — the two liveness probes, the second
/// one behind the auto-registered stack.
async fn liveness(env: &Harness) {
    eprintln!("gap: edge liveness probes");
    let (typed, body) = expect_text(
        env.client
            .get(format!("{}/health", env.base))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /health",
    )
    .await;
    assert_eq!(body, "ok", "/health answers the literal");
    assert!(
        typed.starts_with("text/plain"),
        "/health is plain text: {typed}"
    );

    let auto = json_body(
        env.client
            .get(format!("{}/auto/health", env.base))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /auto/health",
    )
    .await;
    assert_eq!(auto["status"], "ok");
    assert_eq!(auto["routing"], "auto");
}

/// `GET /` — 307 to `/dashboard/`, and the target really answers.
async fn root_redirect(env: &Harness) {
    eprintln!("gap: root redirect");
    // A dedicated no-follow client: the default one would silently chase the
    // redirect and the 307 would never be observed.
    let no_follow = reqwest::Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(20))
        .build()
        .expect("no-follow client");
    let resp = no_follow
        .get(format!("{}/", env.base))
        .send()
        .await
        .expect("GET /");
    assert_eq!(
        resp.status(),
        StatusCode::TEMPORARY_REDIRECT,
        "GET / is a 307, not a 301: a browser re-GETs a rebuilt page"
    );
    assert_eq!(
        resp.headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok()),
        Some("/dashboard/"),
        "the redirect targets the control dashboard"
    );

    // The target answers through the ordinary client.
    let (typed, body) = expect_text(
        env.client
            .get(format!("{}/dashboard/", env.base))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /dashboard/ (the redirect target)",
    )
    .await;
    assert!(
        typed.starts_with("text/html"),
        "the dashboard shell is HTML: {typed}"
    );
    assert!(
        body.to_ascii_lowercase().contains("<html"),
        "the shell carries a real document: {body}"
    );
}

/// The static file plane as it really behaves in the harness: the statics root
/// is the crate directory, which holds no `cyg-client.js`, no `arena_map.json`
/// and no Unity build (`crates/CYGArena/Build/WebGL` does not exist), so every
/// one of these routes is the documented plain-text 404. The baked-in default
/// map on `StaticServer` is empty, so even `/cyg.js`'s fallback misses.
async fn static_plane(env: &Harness) {
    eprintln!("gap: static file plane");
    for (label, path) in [
        ("GET /unity/index.html", "/unity/index.html"),
        ("GET /unity", "/unity"),
        ("GET /Build/game.js", "/Build/game.js"),
        ("GET /TemplateData/style.css", "/TemplateData/style.css"),
        ("GET /cyg.js", "/cyg.js"),
        ("GET /openoxide-client.js", "/openoxide-client.js"),
        ("GET /arena_map.json", "/arena_map.json"),
    ] {
        let (typed, body) = expect_text(
            env.client
                .get(format!("{}{path}", env.base))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            label,
        )
        .await;
        assert_eq!(
            body, "404 not found",
            "{label}: the handler's own miss body"
        );
        assert!(
            typed.starts_with("text/plain"),
            "{label}: the miss is plain text, not HTML: {typed}"
        );
    }
}

/// `GET /dashboard/assets/{name}`: a real asset out of the built dashboard tree.
/// The route maps `/dashboard/assets/<name>` to `dashboard/dist/assets/<name>`,
/// and those names are build-hashed, so the suite enumerates what the server
/// actually built and fetches one of them — the served bytes must equal the file
/// on disk. A name nobody built is the handler's own plain-text 404.
async fn dashboard_assets(env: &Harness) {
    eprintln!("gap: dashboard assets");
    let assets_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../dashboard/dist/assets");
    let mut built: Vec<(String, Vec<u8>)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&assets_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if let Ok(bytes) = std::fs::read(&path) {
                        built.push((name.to_owned(), bytes));
                    }
                }
            }
        }
    }
    built.sort();
    // `dashboard/dist` is a build artefact and is not committed, so the positive
    // read is only asserted when this checkout has one. The miss is always checked.
    let sample = built.into_iter().next();
    if let Some((name, disk)) = sample {
        let resp = env
            .client
            .get(format!("{}/dashboard/assets/{name}", env.base))
            .send()
            .await
            .unwrap();
        let cache = resp
            .headers()
            .get(reqwest::header::CACHE_CONTROL)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let typed = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let served = expect_bytes(
            resp,
            StatusCode::OK,
            &format!("GET /dashboard/assets/{name}"),
        )
        .await;
        assert_eq!(
            served, disk,
            "the served asset is the file the dashboard build wrote"
        );
        assert!(
            typed.starts_with(expected_mime(&name)),
            "{name}: expected {}, got {typed}",
            expected_mime(&name)
        );
        assert!(
            cache.contains("immutable"),
            "hashed dashboard assets are advertised immutable: {cache}"
        );
    } else {
        eprintln!("gap: no dashboard build under {} — serving check skipped", assets_dir.display());
    }

    let (typed, body) = expect_text(
        env.client
            .get(format!("{}/dashboard/assets/nope.svg", env.base))
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "GET /dashboard/assets/{unknown}",
    )
    .await;
    assert_eq!(body, "404 not found");
    assert!(typed.starts_with("text/plain"));
}

/// `POST /matchmake/{method}/{room}` and `POST /api/matchmake/{method}/{room}`:
/// the seat reservation, its reply shape, and every refusal wording the service
/// documents. Returns the seat token, which is also the signal-plane credential.
async fn matchmake_plane(env: &Harness) -> String {
    eprintln!("gap: matchmake");
    let room = format!("gap_room_{}", letters_suffix());

    // The four minting methods, at the short spelling.
    let mut seat_token = String::new();
    for method in ["joinOrCreate", "join", "joinById", "create"] {
        let body = json_body(
            env.client
                .post(format!("{}/matchmake/{method}/{room}", env.base))
                .body("{}")
                .send()
                .await
                .unwrap(),
            StatusCode::OK,
            &format!("POST /matchmake/{method}/{room}"),
        )
        .await;
        assert_eq!(body["name"], room, "{method}: the room name echoes back");
        assert_eq!(body["roomId"], room);
        assert!(
            body["sessionId"].as_str().is_some_and(|id| !id.is_empty()),
            "{method}: a session id: {body}"
        );
        assert!(
            body["processId"]
                .as_str()
                .unwrap_or_default()
                .starts_with("cyg_"),
            "{method}: the host process is named for the plane: {body}"
        );
        assert_eq!(
            body["gamePort"].as_i64(),
            Some(7777),
            "{method}: the game port is the configured one, not the API port"
        );
        assert_eq!(
            body["controlPort"].as_i64(),
            Some(env.port as i64),
            "{method}: the control port is the socket the client just dialled"
        );
        assert!(
            body["udpPort"].as_i64().unwrap_or_default() > 0,
            "{method}: a udp port is offered: {body}"
        );
        // Printed as a JSON string by hand (the reply is `ISeatReservation` plus
        // data-plane fields), so `iceServers` reaches the client as escaped text
        // the page parses itself; the default list is the empty array.
        assert_eq!(
            body["iceServers"].as_str(),
            Some("[]"),
            "{method}: iceServers travels as a JSON string, not an array: {body}"
        );
        let expires = body["expiresAt"].as_u64().unwrap_or_default();
        assert!(
            expires > now_secs(),
            "{method}: the seat expires in the future, not in the past"
        );
        let token = body["seatToken"].as_str().expect("seat token").to_owned();
        assert_eq!(
            body["reconnectionToken"].as_str(),
            Some(token.as_str()),
            "{method}: the seat token and the reconnection token are the one HMAC"
        );
        seat_token = token;
    }

    // The documented spelling answers the same reservation shape.
    json_body(
        env.client
            .post(format!("{}/api/matchmake/create/{room}", env.base))
            .body("{}")
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "POST /api/matchmake/create/{room}",
    )
    .await;

    // `rejoin`/`reconnect` consume the token instead of minting: same session,
    // so a browser reload lands back in its seat.
    for method in ["rejoin", "reconnect"] {
        let resumed = json_body(
            env.client
                .post(format!("{}/matchmake/{method}/{room}", env.base))
                .json(&json!({ "reconnectionToken": seat_token }))
                .send()
                .await
                .unwrap(),
            StatusCode::OK,
            &format!("POST /matchmake/{method} with a live token"),
        )
        .await;
        assert_eq!(resumed["name"], room);
        assert_eq!(resumed["seatToken"].as_str(), Some(seat_token.as_str()));
    }

    // Refusals, each with the handler's exact wording.
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/matchmake/speculate/{room}", env.base))
                .body("{}")
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "an unknown matchmake method",
        )
        .await,
        "no room handler for method 'speculate'"
    );
    for (label, bad_room) in [
        ("a room with a bang", "bad!room"),
        ("a room with a space", "bad%20room"),
        ("a room longer than 64 characters", &"r".repeat(65)),
    ] {
        let body = json_body(
            env.client
                .post(format!("{}/matchmake/create/{bad_room}", env.base))
                .body("{}")
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            label,
        )
        .await;
        assert_eq!(
            body["error"]["message"],
            "invalid room name: use 1-64 letters, digits, '-' or '_'"
        );
        assert_eq!(body["error"]["code"], 3, "400 maps to INVALID_ARGUMENT");
    }
    // The transport cap is enforced before the body is parsed, and an oversized
    // ticket is a bare 413.
    let resp = env
        .client
        .post(format!("{}/matchmake/create/{room}", env.base))
        .body("x".repeat(4097))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::PAYLOAD_TOO_LARGE,
        "a matchmake body over 4096 bytes is refused"
    );
    assert_eq!(
        resp.text().await.unwrap_or_default(),
        "",
        "the ceiling fires before any parsing, so there is nothing to echo"
    );

    // The two reconnect refusals: absent and unknown.
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/matchmake/reconnect/{room}", env.base))
                .body("{}")
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "reconnect with no reconnectionToken",
        )
        .await,
        "missing reconnectionToken"
    );
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/matchmake/reconnect/{room}", env.base))
                .json(&json!({ "reconnectionToken": "not.a.signed-token" }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "reconnect with a forged token",
        )
        .await,
        "reconnection token expired or unknown"
    );

    seat_token
}

/// `GET /api/dashboard/*` — the six unauthenticated reads of the live,
/// in-process registry, checked after the matchmake stage actually minted seats.
async fn dashboard_api(env: &Harness, minted: u64) {
    eprintln!("gap: dashboard api reads the live registry");

    // A fresh database registers no game, so the tenant probe says so plainly.
    let identity = json_body(
        env.client
            .get(format!("{}/api/dashboard/identity", env.base))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /api/dashboard/identity",
    )
    .await;
    assert_eq!(identity["game_id"], Value::Null, "{identity}");
    assert_eq!(identity["game_name"], Value::Null, "{identity}");

    let overview = json_body(
        env.client
            .get(format!("{}/api/dashboard/overview", env.base))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /api/dashboard/overview",
    )
    .await;
    assert_eq!(overview["server"]["name"], GAME_NAME);
    assert_eq!(overview["server"]["port"], 7777);
    assert_eq!(overview["server"]["controlPort"], env.port);
    assert_eq!(overview["rooms"], 1, "this process hosts its one room");
    assert_eq!(
        overview["seats"]["minted"].as_u64().unwrap_or_default() >= minted,
        true,
        "the seat registry counts the mints this suite made: {overview}"
    );
    assert_eq!(overview["seats"]["ttlSecs"], 300, "the harness default TTL");
    assert_eq!(overview["rateLimit"]["rps"], 1000);

    let rooms = json_body(
        env.client
            .get(format!("{}/api/dashboard/rooms", env.base))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /api/dashboard/rooms",
    )
    .await;
    let rows = rooms.as_array().expect("an array of rooms").clone();
    assert_eq!(rows.len(), 1, "{rooms}");
    assert_eq!(rows[0]["name"], GAME_NAME);
    assert_eq!(
        rows[0]["status"], "empty",
        "no client joined through this plane"
    );
    assert_eq!(
        rows[0]["signalingUrl"],
        format!("ws://127.0.0.1:{}/ws/signal", env.port)
    );

    let workers = json_body(
        env.client
            .get(format!("{}/api/dashboard/workers", env.base))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /api/dashboard/workers",
    )
    .await;
    let names: Vec<&str> = workers
        .as_array()
        .expect("an array of workers")
        .iter()
        .filter_map(|row| row["name"].as_str())
        .collect();
    assert!(
        names.contains(&"sim") && names.contains(&"control"),
        "{workers}"
    );

    let clients = json_body(
        env.client
            .get(format!("{}/api/dashboard/clients", env.base))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /api/dashboard/clients",
    )
    .await;
    assert_eq!(
        clients.as_array().map(Vec::len),
        Some(0),
        "no player sessions on this process: {clients}"
    );

    let ticks = json_body(
        env.client
            .get(format!("{}/api/dashboard/ticks", env.base))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /api/dashboard/ticks",
    )
    .await;
    assert!(ticks["points"].is_array(), "a tick series: {ticks}");

    // The tenancy guard: a scoped read for a game this process does not host is
    // refused before the registry is even consulted.
    let stranger = Uuid::new_v4().to_string();
    assert_eq!(
        refusal(
            env.client
                .get(format!(
                    "{}/api/dashboard/overview?game_id={stranger}",
                    env.base
                ))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a scoped overview for an unhosted game",
        )
        .await,
        "This server is not bound to a registered game."
    );
}

/// `WS /ws/dashboard` — hello, then snapshots about once a second, with the
/// live registry inside both frames.
async fn dashboard_socket(env: &Harness) {
    eprintln!("gap: dashboard websocket");
    let mut ws = PlainWs::connect(
        "gap-dashboard",
        format!("ws://127.0.0.1:{}/ws/dashboard", env.port),
    )
    .await;

    let hello = ws.next_json("the hello frame").await;
    assert_eq!(
        hello["type"], "hello",
        "the plane introduces itself: {hello}"
    );
    assert_eq!(hello["protocol"], 1, "the frozen dashboard protocol");
    assert_eq!(hello["overview"]["server"]["name"], GAME_NAME);
    assert!(hello["overview"]["seats"]["minted"].is_number());
    assert!(hello["ticks"]["points"].is_array(), "{hello}");

    let started = std::time::Instant::now();
    let snapshot = ws.next_json("the first snapshot").await;
    assert_eq!(
        snapshot["type"], "snapshot",
        "the tick feed follows: {snapshot}"
    );
    assert_eq!(snapshot["overview"]["server"]["name"], GAME_NAME);
    assert!(
        started.elapsed() >= Duration::from_millis(500),
        "hello arrives at once; a snapshot is a tick, not the handshake"
    );

    ws.shutdown().await;
}

/// `WS /ws/signal` — the seat gate refuses an anonymous dial in words, a forged
/// token in kind, and a real seat token opens the socket with the ICE handout.
async fn signal_socket(env: &Harness, seat_token: &str) {
    eprintln!("gap: signal websocket");

    for (label, query, want) in [
        (
            "no seat token at all",
            "",
            "a seat token from matchmake is required",
        ),
        (
            "a token nobody signed",
            "?seat=not.a.signed-token",
            "seat token is invalid or expired",
        ),
    ] {
        // The gate runs before `on_upgrade`, so the refusal is a plain HTTP
        // answer to the upgrade request — read it with an upgrade-shaped GET.
        let resp = env
            .client
            .get(format!("{}/ws/signal{query}", env.base))
            .header("Upgrade", "websocket")
            .header("Connection", "Upgrade")
            .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
            .header("Sec-WebSocket-Version", "13")
            .send()
            .await
            .unwrap_or_else(|err| panic!("{label}: the upgrade request goes out: {err}"));
        let body = json_body(resp, StatusCode::UNAUTHORIZED, label).await;
        assert_eq!(body["error"]["message"], want, "{label}: {body}");
        assert_eq!(body["error"]["code"], 16, "{label}: 401 is UNAUTHENTICATED");
    }

    // With the matchmake seat, the socket opens and the very first frame is the
    // ICE list the browser needs to gather candidates. The harness's game
    // config carries the default `voice_ice_servers = "[]"`, so the handout is
    // the empty array — emitted as JSON, not as an escaped string.
    let mut ws = PlainWs::connect(
        "gap-signal",
        format!("ws://127.0.0.1:{}/ws/signal?seat={seat_token}", env.port),
    )
    .await;
    let ice = ws.next_json("the ice handout").await;
    assert_eq!(ice["type"], "ice", "the seat's first frame is ICE: {ice}");
    assert_eq!(
        ice["config"],
        json!([]),
        "the config crosses as a JSON value, and the default list is empty"
    );
    ws.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn edge_static_dashboard_and_signal_surfaces_end_to_end() {
    let env = Harness::start().await;

    liveness(&env).await;
    root_redirect(&env).await;
    static_plane(&env).await;
    dashboard_assets(&env).await;

    // Five seats minted (four minting methods plus the `/api` spelling; the two
    // resumes reuse one) — the dashboard reads must count what really happened.
    let seat = matchmake_plane(&env).await;
    dashboard_api(&env, 5).await;
    dashboard_socket(&env).await;
    signal_socket(&env, &seat).await;

    tokio::time::sleep(Duration::from_millis(200)).await;
    env.stop().await;
    eprintln!(
        "gap: liveness, root redirect, static plane, dashboard assets, matchmake, \
         dashboard api, dashboard socket and the signal seat gate passed"
    );
}
