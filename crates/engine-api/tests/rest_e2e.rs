//! The REST surface, end to end: Postgres 16 in a container, the real router on a
//! real socket, and HTTP over TCP. Nothing here is mocked.
//!
//! One test drives the whole lifecycle because the plane has process-wide state by
//! design: the session/console signing keys are installed by the first
//! `ApiState::new` (`OnceLock`), and the console's token-revocation cache is a
//! static. One server and one database keep every assertion about the other honest.
//!
//! The order mirrors a real operator: read telemetry, mint a seat, sign into the
//! console, register games, tune the Unity settings, upload/activate/roll back a
//! room module, and finally authenticate a device account against the same schema.

use std::path::Path;
use std::time::Duration;

use base64::Engine as _;
use engine_bus::Bus;
use engine_config::{ConsoleConfig, ControlConfig, GameConfig, SessionConfig};
use reqwest::multipart::{Form, Part};
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};
use uuid::Uuid;

const CONSOLE_USER: &str = "e2e-admin";
const CONSOLE_PASSWORD: &str = "e2e-password";
const SERVER_KEY: &str = "e2e-server-key";
const GAME_NAME: &str = "e2e-game";

const USERS_SQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../db/schema/users.sql"
));
const USER_DEVICE_SQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../db/schema/user_device.sql"
));
const CONSOLE_USER_SQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../db/schema/console_user.sql"
));
const CONSOLE_AUDIT_SQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../db/schema/console_audit_log.sql"
));

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rest_api_end_to_end() {
    let env = Harness::start().await;
    let base = env.base.clone();
    let client = env.client.clone();

    let port = env.port;
    telemetry(&client, &base, port).await;
    let seat_token = matchmake(&client, &base, port).await;
    let console = console_auth(&client, &base, &env.pool).await;
    let (relay_game, wasm_game) = games(&client, &base, port, &console).await;
    wasm_lifecycle(&client, &base, &console, &relay_game, &wasm_game).await;
    device_auth(&client, &base, &env.pool).await;
    audit_trail(&env.pool).await;
    // The seat is still outstanding at the end of the run, which is what `/status`
    // promised after the mint.
    assert!(!seat_token.is_empty());

    env.stop().await;
    eprintln!("e2e: all REST scenarios passed");
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

struct Harness {
    base: String,
    port: u16,
    client: Client,
    pool: PgPool,
    shutdown: tokio::sync::oneshot::Sender<()>,
    server: tokio::task::JoinHandle<()>,
    container: ContainerAsync<Postgres>,
}

impl Harness {
    async fn start() -> Harness {
        eprintln!("e2e: starting postgres:16-alpine");
        let container = Postgres::default()
            .with_tag("16-alpine")
            .start()
            .await
            .expect("start the Postgres container (is Docker running?)");
        let db_port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("map the Postgres port");
        let dsn = format!("postgres://postgres:postgres@127.0.0.1:{db_port}/postgres");
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .acquire_timeout(Duration::from_secs(10))
            .connect(&dsn)
            .await
            .expect("connect to the container's Postgres");
        apply_schema(&pool).await;

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind an ephemeral API port");
        let port = listener.local_addr().expect("listener address").port();

        let control = ControlConfig {
            api_port: port,
            seat_secret: "e2e-seat-secret".into(),
            // The scenarios share one source address; the bucket must not turn a
            // functional assertion into a 429. Rate limiting is covered by the
            // plane's own unit tests.
            rate_limit_rps: 1000,
            rate_limit_burst: 1000,
            session: SessionConfig {
                database_url: dsn,
                max_db_connections: 8,
                signing_key: "e2e-signing-key".into(),
                refresh_signing_key: "e2e-refresh-signing-key".into(),
                server_key: SERVER_KEY.into(),
                token_expiry_secs: 3_600,
                refresh_expiry_secs: 7_200,
            },
            console: ConsoleConfig {
                username: CONSOLE_USER.into(),
                password: CONSOLE_PASSWORD.into(),
                signing_key: "e2e-console-signing-key".into(),
                token_expiry_secs: 3_600,
            },
            ..ControlConfig::default()
        };
        let game = GameConfig {
            name: GAME_NAME.into(),
            port: 7777,
            max_clients: 16,
            tick_rate: 20,
            ..GameConfig::default()
        };
        let state = engine_api::ApiState::new(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            &game,
            &control,
            Bus::new(64, 64),
            port,
        );

        let (shutdown, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            engine_api::serve(listener, state, async move {
                let _ = shutdown_rx.await;
            })
            .await;
        });

        let base = format!("http://127.0.0.1:{port}");
        let client = Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .expect("build the HTTP client");
        let env = Harness {
            base,
            port,
            client,
            pool,
            shutdown,
            server,
            container,
        };
        env.wait_until_ready().await;
        env
    }

    /// `serve` builds the router (including the tenancy bootstrap) before it accepts
    /// a connection, so a health probe is the honest readiness signal.
    async fn wait_until_ready(&self) {
        for _ in 0..200 {
            if let Ok(resp) = self
                .client
                .get(format!("{}/auto/health", self.base))
                .send()
                .await
                && resp.status().is_success()
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("the API did not answer /auto/health within 10s");
    }

    async fn stop(self) {
        let Harness {
            client,
            shutdown,
            server,
            container,
            ..
        } = self;
        // Drop idle keep-alive sockets before asking for a graceful stop, so the
        // shutdown does not wait on the transport's own connection pool.
        drop(client);
        let _ = shutdown.send(());
        let _ = tokio::time::timeout(Duration::from_secs(10), server).await;
        container.stop().await.ok();
    }
}

async fn apply_schema(pool: &PgPool) {
    // `users_display_name_idx` is a trigram GIN index, and the committed schema
    // files are plain `CREATE TABLE`s. Both facts are the production contract, so
    // the harness runs the extension first and the files verbatim.
    sqlx::raw_sql("CREATE EXTENSION IF NOT EXISTS pg_trgm;")
        .execute(pool)
        .await
        .expect("create pg_trgm");
    for (label, ddl) in [
        ("users", USERS_SQL),
        ("user_device", USER_DEVICE_SQL),
        ("console_user", CONSOLE_USER_SQL),
        ("console_audit_log", CONSOLE_AUDIT_SQL),
    ] {
        apply_file(pool, label, ddl).await;
    }
}

/// The dumps restate `CREATE UNIQUE INDEX` for indexes their own named `UNIQUE`
/// constraints already created. Applying the whole file in one batch would abort on
/// the first such restatement and skip everything after it (including the trigram
/// index), so statements run one at a time and only a duplicate-relation error is
/// tolerated — the table and the constraint it duplicates are already there.
async fn apply_file(pool: &PgPool, label: &str, ddl: &str) {
    for statement in statements(ddl) {
        if let Err(err) = sqlx::raw_sql(&statement).execute(pool).await {
            let duplicate = matches!(
                &err,
                sqlx::Error::Database(db)
                    if matches!(db.code().as_deref(), Some("42P07" | "42710"))
            );
            if duplicate {
                continue;
            }
            panic!("apply {label}.sql: {err}\nstatement: {statement}");
        }
    }
}

/// Split a dump into statements on `;`, ignoring separators inside single quotes,
/// double quotes, line comments and dollar-quoted bodies.
fn statements(sql: &str) -> Vec<String> {
    let bytes = sql.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    let mut dollar_tag: Option<Vec<u8>> = None;
    while i < bytes.len() {
        if let Some(tag) = &dollar_tag {
            if bytes[i..].starts_with(tag) {
                i += tag.len();
                dollar_tag = None;
            } else {
                i += 1;
            }
            continue;
        }
        match bytes[i] {
            b'\'' => {
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\'' {
                        if bytes.get(i + 1) == Some(&b'\'') {
                            i += 2;
                            continue;
                        }
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += 1;
                }
                i += 1;
            }
            b'-' if bytes.get(i + 1) == Some(&b'-') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'$' => {
                let mut end = i + 1;
                while end < bytes.len()
                    && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_')
                {
                    end += 1;
                }
                if bytes.get(end) == Some(&b'$') {
                    dollar_tag = Some(bytes[i..=end].to_vec());
                    i = end + 1;
                } else {
                    i += 1;
                }
            }
            b';' => {
                let statement = sql[start..=i].trim();
                if !statement.is_empty() {
                    out.push(statement.to_owned());
                }
                start = i + 1;
                i += 1;
            }
            _ => i += 1,
        }
    }
    let tail = sql[start..].trim();
    if !tail.is_empty() {
        out.push(tail.to_owned());
    }
    out
}

// ---------------------------------------------------------------------------
// HTTP helpers
// ---------------------------------------------------------------------------

async fn expect(resp: reqwest::Response, status: StatusCode, what: &str) -> String {
    let actual = resp.status();
    let body = resp.text().await.unwrap_or_default();
    assert_eq!(
        actual, status,
        "{what}: expected {status}, got {actual}: {body}"
    );
    body
}

async fn json_body(resp: reqwest::Response, status: StatusCode, what: &str) -> Value {
    let body = expect(resp, status, what).await;
    serde_json::from_str(&body)
        .unwrap_or_else(|err| panic!("{what}: response is not JSON ({err}): {body}"))
}

/// The `/v2` refusal envelope, checked for shape and message rather than only status.
async fn refusal(resp: reqwest::Response, status: StatusCode, what: &str) -> String {
    let body = json_body(resp, status, what).await;
    body["error"]["message"]
        .as_str()
        .unwrap_or_else(|| panic!("{what}: refusal has no message: {body}"))
        .to_owned()
}

async fn console_login(client: &Client, base: &str) -> String {
    let body = json_body(
        client
            .post(format!("{base}/v2/console/authenticate"))
            .json(&json!({ "username": CONSOLE_USER, "password": CONSOLE_PASSWORD }))
            .send()
            .await
            .expect("console sign-in"),
        StatusCode::OK,
        "console sign-in",
    )
    .await;
    assert_eq!(body["username"], CONSOLE_USER);
    assert_eq!(body["role"], "admin");
    body["token"].as_str().expect("console token").to_owned()
}

// ---------------------------------------------------------------------------
// Scenarios
// ---------------------------------------------------------------------------

async fn telemetry(client: &Client, base: &str, port: u16) {
    eprintln!("e2e: telemetry");
    let health = json_body(
        client
            .get(format!("{base}/auto/health"))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /auto/health",
    )
    .await;
    assert_eq!(health["status"], "ok");
    assert_eq!(health["routing"], "auto");

    let status = json_body(
        client.get(format!("{base}/status")).send().await.unwrap(),
        StatusCode::OK,
        "GET /status",
    )
    .await;
    assert_eq!(status["plane"]["controlPort"], port);
    assert_eq!(status["plane"]["game"]["name"], GAME_NAME);
    assert_eq!(status["plane"]["game"]["port"], 7777);
    assert_eq!(status["seats"]["minted"], 0);
    assert_eq!(status["seats"]["outstanding"], 0);
    assert_eq!(status["rateLimit"]["rps"], 1000);
    assert!(
        status["sim"].is_null(),
        "no sim is attached in this harness"
    );

    let rooms = json_body(
        client
            .get(format!("{base}/api/rooms"))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /api/rooms",
    )
    .await;
    let rooms = rooms.as_array().expect("/api/rooms is a list");
    assert_eq!(rooms.len(), 1);
    let room = &rooms[0];
    assert_eq!(room["name"], GAME_NAME);
    assert_eq!(room["maxClients"], 16);
    assert_eq!(room["controlPort"], port);
    assert_eq!(room["port"], 7777);
    assert_eq!(room["hasSimSample"], false);
    assert!(
        room["ageMs"].is_null(),
        "no sample means the age is unknown"
    );
}

/// Mint a seat, resume it, and prove the transport-level refusals are distinct.
async fn matchmake(client: &Client, base: &str, port: u16) -> String {
    eprintln!("e2e: matchmake");
    let seat = json_body(
        client
            .post(format!("{base}/api/matchmake/joinOrCreate/e2e-room"))
            .body("{}")
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "matchmake joinOrCreate",
    )
    .await;
    assert_eq!(seat["name"], "e2e-room");
    assert_eq!(seat["roomId"], "e2e-room");
    assert_eq!(seat["controlPort"], port);
    assert_eq!(seat["gamePort"], 7777);
    let session_id = seat["sessionId"].as_str().expect("sessionId").to_owned();
    assert!(!session_id.is_empty());
    let token = seat["reconnectionToken"]
        .as_str()
        .expect("reconnectionToken")
        .to_owned();
    assert_eq!(seat["seatToken"], token);
    // `iceServers` is a JSON document nested as a string, which is what browsers
    // parse a second time.
    let ice: Value = serde_json::from_str(seat["iceServers"].as_str().unwrap())
        .expect("iceServers is JSON text");
    assert!(ice.is_array());
    assert!(seat["udpPort"].as_u64().is_some_and(|p| p > 0));

    // The second mounted spelling, and a different seat: minting is per request.
    let other = json_body(
        client
            .post(format!("{base}/matchmake/join/create"))
            .body("{}")
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "matchmake via /matchmake",
    )
    .await;
    assert_eq!(other["name"], "create");
    assert_ne!(other["sessionId"], session_id);

    // A reconnect consumes the token: same session, same seat.
    let resumed = json_body(
        client
            .post(format!("{base}/api/matchmake/reconnect/e2e-room"))
            .json(&json!({ "reconnectionToken": token }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "reconnect",
    )
    .await;
    assert_eq!(resumed["sessionId"], session_id);
    assert_eq!(resumed["reconnectionToken"], token);

    let unknown = refusal(
        client
            .post(format!("{base}/api/matchmake/reconnect/e2e-room"))
            .json(&json!({ "reconnectionToken": "not-a-seat" }))
            .send()
            .await
            .unwrap(),
        StatusCode::UNAUTHORIZED,
        "reconnect with a bad token",
    )
    .await;
    assert!(unknown.contains("expired or unknown"), "{unknown}");

    expect(
        client
            .post(format!("{base}/api/matchmake/teleport/e2e-room"))
            .body("{}")
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "unknown matchmake method",
    )
    .await;
    expect(
        client
            .post(format!("{base}/api/matchmake/joinOrCreate/bad%20room"))
            .body("{}")
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "invalid room name",
    )
    .await;
    expect(
        client
            .post(format!("{base}/api/matchmake/joinOrCreate/e2e-room"))
            .body("x".repeat(4097))
            .send()
            .await
            .unwrap(),
        StatusCode::PAYLOAD_TOO_LARGE,
        "oversized matchmake body",
    )
    .await;

    let status = json_body(
        client.get(format!("{base}/status")).send().await.unwrap(),
        StatusCode::OK,
        "GET /status after matchmake",
    )
    .await;
    assert_eq!(status["seats"]["minted"], 2);
    assert!(status["seats"]["outstanding"].as_u64().unwrap() >= 2);

    token
}

/// Sign in, browse, log out, and prove the revoked token is dead.
async fn console_auth(client: &Client, base: &str, pool: &PgPool) -> String {
    eprintln!("e2e: console auth");
    let wrong = refusal(
        client
            .post(format!("{base}/v2/console/authenticate"))
            .json(&json!({ "username": CONSOLE_USER, "password": "wrong" }))
            .send()
            .await
            .unwrap(),
        StatusCode::UNAUTHORIZED,
        "console sign-in with a wrong password",
    )
    .await;
    assert_eq!(wrong, "Invalid credentials.");

    expect(
        client
            .get(format!("{base}/v2/console/games"))
            .send()
            .await
            .unwrap(),
        StatusCode::UNAUTHORIZED,
        "console list without a token",
    )
    .await;

    let token = console_login(client, base).await;
    let list = json_body(
        client
            .get(format!("{base}/v2/console/games"))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "console list with a token",
    )
    .await;
    assert_eq!(list["total"], 0);

    // Sign-in is audited through the real table, not a counter.
    let logins: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM console_audit_log WHERE action = 'authenticate.login'",
    )
    .fetch_one(pool)
    .await
    .expect("read the audit trail");
    assert!(logins >= 1, "the console sign-in must leave an audit row");

    expect(
        client
            .post(format!("{base}/v2/console/authenticate/logout"))
            .bearer_auth(&token)
            .json(&json!({}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "console logout",
    )
    .await;
    expect(
        client
            .get(format!("{base}/v2/console/games"))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap(),
        StatusCode::UNAUTHORIZED,
        "console list after logout",
    )
    .await;

    // The revoked token is dead; the rest of the run signs in fresh.
    console_login(client, base).await
}

/// Games CRUD, the Unity settings download, and the archive lifecycle. Returns the
/// relay game (Unity) and the authoritative game (custom/WASM) the next scenario uses.
async fn games(client: &Client, base: &str, port: u16, console: &str) -> (String, String) {
    eprintln!("e2e: games and unity settings");
    let suffix = &Uuid::new_v4().simple().to_string()[..8];
    let unity_name = format!("E2E Unity {suffix}");
    let unity = json_body(
        client
            .post(format!("{base}/v2/console/games"))
            .bearer_auth(console)
            .json(&json!({ "name": unity_name, "engine_type": "unity" }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "create a Unity game",
    )
    .await;
    assert_eq!(unity["engine_type"], "unity");
    assert_eq!(unity["backend_mode"], "relay");
    assert_eq!(unity["status"], "active");
    assert!(unity["game_key"].as_str().unwrap().starts_with("gm_"));
    assert!(
        unity["client_key"]
            .as_str()
            .unwrap()
            .starts_with("cyg_pub_")
    );
    assert!(
        unity.get("api_key").is_none(),
        "the secret belongs to the detail view only"
    );
    assert_eq!(unity["config"]["tick_rate"], 128);
    assert_eq!(unity["config"]["max_clients"], 16);
    let unity_id = unity["id"].as_str().expect("game id").to_owned();

    let wasm_name = format!("E2E Custom {suffix}");
    let wasm_game = json_body(
        client
            .post(format!("{base}/v2/console/games"))
            .bearer_auth(console)
            .json(&json!({
                "name": wasm_name,
                "engine_type": "custom",
                // Flat fields are the product plan's spelling and win over `config`.
                "tick_rate": 60,
                "max_clients": 32,
                "persistent_rooms": true
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "create a custom (WASM) game",
    )
    .await;
    assert_eq!(wasm_game["engine_type"], "custom");
    assert_eq!(wasm_game["backend_mode"], "wasm");
    assert_eq!(wasm_game["config"]["tick_rate"], 60);
    assert_eq!(wasm_game["config"]["max_clients"], 32);
    assert_eq!(wasm_game["config"]["persistent_rooms"], true);
    let wasm_id = wasm_game["id"].as_str().expect("game id").to_owned();

    // A create names a name, a known engine, and an object config — nothing else.
    refusal(
        client
            .post(format!("{base}/v2/console/games"))
            .bearer_auth(console)
            .json(&json!({ "name": "   " }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "create with a blank name",
    )
    .await;
    refusal(
        client
            .post(format!("{base}/v2/console/games"))
            .bearer_auth(console)
            .json(&json!({ "name": "Frostbite", "engine_type": "frostbite" }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "create with an unknown engine",
    )
    .await;
    refusal(
        client
            .post(format!("{base}/v2/console/games"))
            .bearer_auth(console)
            .json(&json!({ "name": "Bad Config", "config": [1, 2] }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "create with a non-object config",
    )
    .await;

    let listing = json_body(
        client
            .get(format!("{base}/v2/console/games"))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "list games",
    )
    .await;
    assert_eq!(listing["total"], 2);
    assert!(
        listing["games"]
            .as_array()
            .unwrap()
            .iter()
            .any(|game| game["id"] == unity_id)
    );

    let detail = json_body(
        client
            .get(format!("{base}/v2/console/games/{unity_id}"))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "game detail",
    )
    .await;
    let api_key = detail["api_key"].as_str().expect("api_key").to_owned();
    assert!(api_key.starts_with("cyg_sec_"));
    assert_eq!(detail["module_count"], 0);
    assert!(detail["active_module_version"].is_null());

    let renamed = format!("E2E Unity Renamed {suffix}");
    let updated = json_body(
        client
            .put(format!("{base}/v2/console/games/{unity_id}"))
            .bearer_auth(console)
            .json(&json!({ "name": renamed, "status": "suspended" }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "patch a game",
    )
    .await;
    assert_eq!(updated["name"], renamed);
    assert_eq!(updated["status"], "suspended");
    assert_eq!(updated["backend_mode"], "relay");

    refusal(
        client
            .put(format!("{base}/v2/console/games/{unity_id}"))
            .bearer_auth(console)
            .json(&json!({}))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "empty game patch",
    )
    .await;
    refusal(
        client
            .put(format!("{base}/v2/console/games/{}", Uuid::new_v4()))
            .bearer_auth(console)
            .json(&json!({ "name": "ghost" }))
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "patch an unknown game",
    )
    .await;

    // `unity-settings` is the SDK's connection companion: camelCase, the runtime
    // values out of `games.config`, and the secret only on request.
    let settings = json_body(
        client
            .get(format!(
                "{base}/v2/console/games/{unity_id}/unity-settings?format=json"
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "unity settings JSON",
    )
    .await;
    assert_eq!(settings["gameKey"], unity["game_key"]);
    assert_eq!(settings["clientKey"], unity["client_key"]);
    assert!(settings.get("apiKey").is_none());
    assert_eq!(
        settings["serverHttpUrl"],
        format!("http://127.0.0.1:{port}")
    );
    assert_eq!(settings["serverWsPath"], "/ws");
    assert_eq!(settings["defaultRoom"], "cyg_room");
    assert_eq!(settings["tickRate"], 128);
    assert_eq!(settings["maxPlayers"], 16);
    assert_eq!(settings["persistentRooms"], false);
    assert_eq!(settings["backendMode"], "relay");

    let revealed = json_body(
        client
            .get(format!(
                "{base}/v2/console/games/{unity_id}/unity-settings?format=json&reveal=true"
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "unity settings JSON with reveal",
    )
    .await;
    assert_eq!(revealed["apiKey"], api_key);

    let asset_resp = client
        .get(format!("{base}/v2/console/games/{unity_id}/unity-settings"))
        .bearer_auth(console)
        .send()
        .await
        .unwrap();
    assert_eq!(asset_resp.status(), StatusCode::OK);
    assert_eq!(
        asset_resp.headers()["content-type"],
        "application/octet-stream"
    );
    assert!(
        asset_resp.headers()["content-disposition"]
            .to_str()
            .unwrap()
            .contains("CYGServerSettings.asset")
    );
    let asset = asset_resp.text().await.unwrap();
    assert!(asset.contains("m_Name: CYGServerSettings"), "{asset}");
    assert!(asset.contains(&format!("serverHttpUrl: http://127.0.0.1:{port}")));
    assert!(asset.contains("serverWsPath: /ws"));
    assert!(asset.contains("roomName: cyg_room"));
    assert!(asset.contains("maxPlayers: 16"));
    assert!(asset.contains("protocolVersion: 1"));
    assert!(asset.contains("matchmakeMethod: joinOrCreate"));

    refusal(
        client
            .get(format!(
                "{base}/v2/console/games/{unity_id}/unity-settings?format=xml"
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "unknown unity-settings format",
    )
    .await;

    // Archive is a soft delete: the row and its key survive for history.
    let archived = json_body(
        client
            .delete(format!("{base}/v2/console/games/{unity_id}"))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "archive a game",
    )
    .await;
    assert_eq!(archived["archived"], true);
    let after = json_body(
        client
            .get(format!("{base}/v2/console/games/{unity_id}"))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "read an archived game",
    )
    .await;
    assert_eq!(after["status"], "archived");
    refusal(
        client
            .delete(format!("{base}/v2/console/games/{}", Uuid::new_v4()))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "archive an unknown game",
    )
    .await;

    (unity_id, wasm_id)
}

/// The WASM room-module store: ABI-gated uploads, versions, activation, rollback,
/// the active-version delete refusal, and the relay-mode gate.
async fn wasm_lifecycle(
    client: &Client,
    base: &str,
    console: &str,
    relay_game: &str,
    wasm_game: &str,
) {
    eprintln!("e2e: wasm module lifecycle");
    let wasm = module_bytes();
    let upload_url = |path: &str| format!("{base}/v2/console/games/{wasm_game}/wasm{path}");

    let form = Form::new()
        .part(
            "file",
            Part::bytes(wasm.clone()).file_name("room_logic.wasm"),
        )
        .text("source_code", "// e2e room module");
    let first = json_body(
        client
            .post(upload_url("/upload"))
            .bearer_auth(console)
            .multipart(form)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "upload the first module",
    )
    .await;
    assert_eq!(first["abi"]["valid"], true);
    assert_eq!(first["abi"]["wasm_magic_ok"], true);
    assert_eq!(first["abi"]["exports_missing"].as_array().unwrap().len(), 0);
    assert_eq!(
        first["abi"]["exports_found"].as_array().unwrap().len(),
        engine_api::services::wasm::REQUIRED_EXPORTS.len()
    );
    assert_eq!(first["module"]["version"], 1);
    assert_eq!(first["module"]["is_active"], true);
    assert_eq!(first["module"]["filename"], "room_logic.wasm");
    assert_eq!(first["module"]["has_source"], true);
    assert_eq!(first["module"]["size_bytes"], wasm.len());
    assert_eq!(first["module"]["sha256"].as_str().unwrap().len(), 64);

    // The ABI gate runs before the row is written: the listing stays at one.
    let broken = Form::new().part(
        "file",
        Part::bytes(b"definitely not wasm".to_vec()).file_name("broken.wasm"),
    );
    let message = refusal(
        client
            .post(upload_url("/upload"))
            .bearer_auth(console)
            .multipart(broken)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "upload non-wasm bytes",
    )
    .await;
    assert!(message.contains("WebAssembly"), "{message}");

    let listing = json_body(
        client
            .get(upload_url(""))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "list modules",
    )
    .await;
    assert_eq!(listing.as_array().unwrap().len(), 1);

    // The collection path is the same upload: a POST to `…/wasm` creates version 2.
    let second_form = Form::new().part(
        "file",
        Part::bytes(wasm.clone()).file_name("room_logic_v2.wasm"),
    );
    let second = json_body(
        client
            .post(upload_url(""))
            .bearer_auth(console)
            .multipart(second_form)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "upload the second module",
    )
    .await;
    assert_eq!(second["module"]["version"], 2);
    assert_eq!(second["module"]["is_active"], true);
    assert_eq!(second["module"]["has_source"], false);

    let listing = json_body(
        client
            .get(upload_url(""))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "list modules after upload",
    )
    .await;
    let listing = listing.as_array().unwrap();
    assert_eq!(listing.len(), 2);
    assert_eq!(listing[0]["version"], 2);
    assert_eq!(listing[1]["version"], 1);
    assert_eq!(listing[0]["is_active"], true);
    assert_eq!(listing[1]["is_active"], false);

    let activated = json_body(
        client
            .post(upload_url("/activate"))
            .bearer_auth(console)
            .json(&json!({ "version": 1 }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "activate version 1",
    )
    .await;
    assert_eq!(activated["version"], 1);
    assert_eq!(activated["is_active"], true);

    let rolled_back = json_body(
        client
            .post(upload_url("/rollback"))
            .bearer_auth(console)
            .json(&json!({ "version": 2 }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "roll forward via rollback",
    )
    .await;
    assert_eq!(rolled_back["version"], 2);
    assert_eq!(rolled_back["is_active"], true);

    let detail = json_body(
        client
            .get(format!("{base}/v2/console/games/{wasm_game}"))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "game detail after activation",
    )
    .await;
    assert_eq!(detail["active_module_version"], 2);
    assert_eq!(detail["module_count"], 2);

    let message = refusal(
        client
            .delete(upload_url("/2"))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        // `FAILED_PRECONDITION` (gRPC 9) rides on HTTP 400 in this error table.
        StatusCode::BAD_REQUEST,
        "delete the active module",
    )
    .await;
    assert!(message.contains("active module"), "{message}");

    let body = expect(
        client
            .delete(upload_url("/1"))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "delete an inactive module",
    )
    .await;
    assert!(body.is_empty());

    let listing = json_body(
        client
            .get(upload_url(""))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "list modules after delete",
    )
    .await;
    let listing = listing.as_array().unwrap();
    assert_eq!(listing.len(), 1);
    assert_eq!(listing[0]["version"], 2);

    refusal(
        client
            .post(upload_url("/activate"))
            .bearer_auth(console)
            .json(&json!({ "version": 9 }))
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "activate a missing version",
    )
    .await;

    // A relay game has no server-side module, so the gate refuses the upload.
    let relay_form = Form::new().part(
        "file",
        Part::bytes(wasm.clone()).file_name("room_logic.wasm"),
    );
    let message = refusal(
        client
            .post(format!("{base}/v2/console/games/{relay_game}/wasm/upload"))
            .bearer_auth(console)
            .multipart(relay_form)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "upload to a relay game",
    )
    .await;
    assert!(message.contains("Unity C# Relay"), "{message}");

    expect(
        client
            .delete(format!("{base}/v2/console/games/not-a-uuid/wasm/1"))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "delete from a malformed game id",
    )
    .await;
    expect(
        client
            .delete(format!("{base}/v2/console/games/{}/wasm/1", Uuid::new_v4()))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "delete from an unknown game",
    )
    .await;
}

/// `/v2/account/authenticate/device` against the same container: the Basic server
/// key, the create-then-resume path, and the rows both leave behind.
async fn device_auth(client: &Client, base: &str, pool: &PgPool) {
    eprintln!("e2e: device authentication");
    let basic = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{SERVER_KEY}:"))
    );
    let device_id = format!("e2e-device-{}", Uuid::new_v4());
    let username = format!("e2e-player-{}", &Uuid::new_v4().simple().to_string()[..8]);
    let url = format!("{base}/v2/account/authenticate/device?username={username}&create=true");

    let created = json_body(
        client
            .post(&url)
            .header(reqwest::header::AUTHORIZATION, &basic)
            .json(&json!({ "id": device_id, "vars": { "platform": "e2e" } }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "device authenticate (create)",
    )
    .await;
    assert_eq!(created["created"], true);
    let uid = jwt_uid(&created["token"]);
    assert!(!created["refresh_token"].as_str().unwrap().is_empty());

    // The same device again resumes the account; `created` is omitted when false.
    let resumed = json_body(
        client
            .post(format!(
                "{base}/v2/account/authenticate/device?username={username}"
            ))
            .header(reqwest::header::AUTHORIZATION, &basic)
            .json(&json!({ "id": device_id }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "device authenticate (resume)",
    )
    .await;
    assert!(resumed.get("created").is_none());
    assert_eq!(jwt_uid(&resumed["token"]), uid);
    assert_ne!(resumed["token"], created["token"]);

    refusal(
        client
            .post(format!(
                "{base}/v2/account/authenticate/device?username={username}"
            ))
            .header(
                reqwest::header::AUTHORIZATION,
                format!(
                    "Basic {}",
                    base64::engine::general_purpose::STANDARD.encode("wrong-key:")
                ),
            )
            .json(&json!({ "id": device_id }))
            .send()
            .await
            .unwrap(),
        StatusCode::UNAUTHORIZED,
        "device authenticate with a wrong server key",
    )
    .await;
    refusal(
        client
            .post(format!(
                "{base}/v2/account/authenticate/device?username={username}"
            ))
            .json(&json!({ "id": device_id }))
            .send()
            .await
            .unwrap(),
        StatusCode::UNAUTHORIZED,
        "device authenticate without a server key",
    )
    .await;

    // Persistence is the point: both rows exist and name the same account.
    let stored_device: Uuid = sqlx::query_scalar("SELECT user_id FROM user_device WHERE id = $1")
        .bind(&device_id)
        .fetch_one(pool)
        .await
        .expect("the device row was written");
    assert_eq!(stored_device.to_string(), uid);
    let stored_username: String = sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
        .bind(stored_device)
        .fetch_one(pool)
        .await
        .expect("the user row was written");
    assert_eq!(stored_username, username);
}

async fn audit_trail(pool: &PgPool) {
    eprintln!("e2e: audit trail");
    let rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM console_audit_log WHERE action = 'authenticate.login'",
    )
    .fetch_one(pool)
    .await
    .expect("read the audit trail");
    assert!(rows >= 2, "two successful sign-ins, two audit rows: {rows}");
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn jwt_uid(token: &Value) -> String {
    let token = token.as_str().expect("session token");
    let payload = token.split('.').nth(1).expect("JWT payload segment");
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .expect("base64url JWT payload");
    let claims: Value = serde_json::from_slice(&decoded).expect("JWT claims JSON");
    claims["uid"].as_str().expect("uid claim").to_owned()
}

/// A hand-assembled module exporting exactly the ABI the host declares. The same
/// builder the unit tests use: magic, one `() -> ()` type, one function per export,
/// and empty bodies.
fn module_bytes() -> Vec<u8> {
    let names = engine_api::services::wasm::REQUIRED_EXPORTS;
    let mut module = vec![0x00, b'a', b's', b'm', 0x01, 0x00, 0x00, 0x00];

    let mut types = leb128(1);
    types.extend([0x60u8, 0x00, 0x00]);
    module.extend(section(1, &types));

    let functions: Vec<u8> = leb128(names.len())
        .into_iter()
        .chain(std::iter::repeat_n(0u8, names.len()))
        .collect();
    module.extend(section(3, &functions));

    let mut exports = leb128(names.len());
    for (index, name) in names.iter().enumerate() {
        exports.extend(leb128(name.len()));
        exports.extend(name.as_bytes());
        exports.push(0x00);
        exports.extend(leb128(index));
    }
    module.extend(section(7, &exports));

    let mut bodies = leb128(names.len());
    for _ in names {
        bodies.extend(leb128(2));
        bodies.extend([0x00, 0x0b]);
    }
    module.extend(section(10, &bodies));
    module
}

fn leb128(mut value: usize) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            return out;
        }
    }
}

fn section(id: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![id];
    out.extend(leb128(payload.len()));
    out.extend(payload);
    out
}
