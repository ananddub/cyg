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
//! room module, authenticate a device account, and finally run a tournament from
//! definition through join, score writes and deletion.

use std::path::Path;
use std::time::Duration;

use base64::Engine as _;
use chrono::{Duration as ChronoDuration, Utc};
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

/// An 8-character random suffix of hex LETTERS only (`a`-`f`).
///
/// The moderation filter rejects ~8% of random digit+letter strings — its
/// context matcher reads sequences like "69a8e" as evasive sexual references
/// — so e2e identifiers that pass through a profanity check must not sample
/// digits. Letters-only suffixes are immune.
fn letters_suffix() -> String {
    let mut out = String::new();
    let mut uuid = Uuid::new_v4().simple().to_string().into_bytes();
    while out.len() < 8 {
        if uuid.is_empty() {
            uuid = Uuid::new_v4().simple().to_string().into_bytes();
        }
        let c = uuid.remove(0) as char;
        if c.is_ascii_alphabetic() {
            out.push(c);
        }
    }
    out
}


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
const LEADERBOARD_SQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../db/schema/leaderboard.sql"
));
const LEADERBOARD_RECORD_SQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../db/schema/leaderboard_record.sql"
));
const USER_EDGE_SQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../db/schema/user_edge.sql"
));
const USER_TOMBSTONE_SQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../db/schema/user_tombstone.sql"
));
const USERS_NOTES_SQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../db/schema/users_notes.sql"
));
const WALLET_LEDGER_SQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../db/schema/wallet_ledger.sql"
));
const GROUPS_SQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../db/schema/groups.sql"
));
const GROUP_EDGE_SQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../db/schema/group_edge.sql"
));
const MESSAGE_SQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../db/schema/message.sql"
));
const NOTIFICATION_SQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../db/schema/notification.sql"
));
const STORAGE_SQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../db/schema/storage.sql"
));
const STORAGE_INDEX_SQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../db/schema/storage_index.sql"
));
const PURCHASE_SQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../db/schema/purchase.sql"
));
const SUBSCRIPTION_SQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../db/schema/subscription.sql"
));
const SETTING_SQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../db/schema/setting.sql"
));
const CONSOLE_ACL_TEMPLATE_SQL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../db/schema/console_acl_template.sql"
));

/// The inventory repositories are the only queries with no committed schema
/// file; the tables live in the runtime migration. These are their columns.
const INVENTORY_SQL: &str = "
CREATE TABLE IF NOT EXISTS item_definitions (
    item_id text PRIMARY KEY,
    name text NOT NULL,
    category text NOT NULL DEFAULT 'misc',
    max_stack integer NOT NULL DEFAULT 1,
    rarity text NOT NULL DEFAULT 'common',
    properties jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT NOW()
);
CREATE TABLE IF NOT EXISTS player_inventory (
    instance_id uuid PRIMARY KEY,
    user_id uuid NOT NULL,
    item_id text NOT NULL,
    quantity integer NOT NULL DEFAULT 1,
    slot_index integer NOT NULL DEFAULT -1,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT NOW(),
    updated_at timestamptz NOT NULL DEFAULT NOW()
);
";

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
    auth_surface(&client, &base, &env.pool).await;
    account_surface(&client, &base, &env.pool).await;
    link_surface(&client, &base, &env.pool).await;
    user_surface(&client, &base, &env.pool).await;
    tournaments(&client, &base, &console, &env.pool).await;
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
        ("leaderboard", LEADERBOARD_SQL),
        // After `leaderboard`, whose primary key its foreign key names.
        ("leaderboard_record", LEADERBOARD_RECORD_SQL),
        // Every file below with a foreign key names `users(id)`.
        ("user_edge", USER_EDGE_SQL),
        ("user_tombstone", USER_TOMBSTONE_SQL),
        ("users_notes", USERS_NOTES_SQL),
        ("wallet_ledger", WALLET_LEDGER_SQL),
        ("groups", GROUPS_SQL),
        ("group_edge", GROUP_EDGE_SQL),
        ("message", MESSAGE_SQL),
        ("notification", NOTIFICATION_SQL),
        ("storage", STORAGE_SQL),
        ("storage_index", STORAGE_INDEX_SQL),
        ("purchase", PURCHASE_SQL),
        ("subscription", SUBSCRIPTION_SQL),
        ("setting", SETTING_SQL),
        ("console_acl_template", CONSOLE_ACL_TEMPLATE_SQL),
        ("inventory", INVENTORY_SQL),
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

/// `Basic base64("<server key>:")` — the credential the player auth plane wants.
fn server_key_header() -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{SERVER_KEY}:"))
    )
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
    let suffix = letters_suffix();
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
    let username = format!("e2e-player-{}", letters_suffix());
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

/// A signed-in test player: the token pair plus the identifiers the server wrote,
/// so record rows and follow-up routes can be checked against that account.
struct Player {
    token: String,
    refresh_token: String,
    uid: String,
    username: String,
    device_id: String,
}

/// A fresh device account through the real `/v2/account/authenticate/device`
/// route, with the Basic server key the plane was booted with.
async fn device_player(client: &Client, base: &str, label: &str) -> Player {
    let device_id = format!("e2e-{label}-{}", Uuid::new_v4());
    let username = format!("e2e-{label}-{}", letters_suffix());
    let body = json_body(
        client
            .post(format!(
                "{base}/v2/account/authenticate/device?username={username}&create=true"
            ))
            .header(reqwest::header::AUTHORIZATION, server_key_header())
            .json(&json!({ "id": device_id }))
            .send()
            .await
            .expect("device authenticate for a tournament player"),
        StatusCode::OK,
        "device authenticate for a tournament player",
    )
    .await;
    Player {
        token: body["token"].as_str().expect("session token").to_owned(),
        refresh_token: body["refresh_token"]
            .as_str()
            .expect("refresh token")
            .to_owned(),
        uid: jwt_uid(&body["token"]),
        username,
        device_id,
    }
}

/// Define a tournament through the console route and return its wire shape.
async fn define_tournament(client: &Client, base: &str, console: &str, body: Value) -> Value {
    json_body(
        client
            .post(format!("{base}/v2/console/tournament"))
            .bearer_auth(console)
            .json(&body)
            .send()
            .await
            .expect("define a tournament"),
        StatusCode::OK,
        "define a tournament",
    )
    .await
}

/// Submit one score to a tournament as a player.
async fn write_record(client: &Client, base: &str, token: &str, id: &str, body: Value) -> Value {
    json_body(
        client
            .post(format!("{base}/v2/tournament/{id}"))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .expect("write a tournament record"),
        StatusCode::OK,
        "write a tournament record",
    )
    .await
}

fn tournament_ids(page: &Value) -> Vec<String> {
    page["tournaments"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|row| row["id"].as_str().expect("tournament id").to_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// Tournament behaviour over the real routes: the console defines the boards, a
/// player browses, joins, writes, reads the round and its standing, and the
/// operator renames and deletes. Window maths, join gates, size and attempt
/// counters, record ordering and cascading deletes are asserted against the
/// database the routes actually wrote.
async fn tournaments(client: &Client, base: &str, console: &str, pool: &PgPool) {
    eprintln!("e2e: tournaments");
    let now = Utc::now();
    let stamp = |offset: i64| (now + ChronoDuration::seconds(offset)).to_rfc3339();
    let suffix = letters_suffix();
    let joined_id = format!("e2e-tournament-joined-{suffix}");
    let open_id = format!("e2e-tournament-open-{suffix}");
    let ended_id = format!("e2e-tournament-ended-{suffix}");
    let authoritative_id = format!("e2e-tournament-authoritative-{suffix}");
    let plain_id = format!("e2e-leaderboard-{suffix}");

    let alpha = device_player(client, base, "alpha").await;
    let bravo = device_player(client, base, "bravo").await;
    let charlie = device_player(client, base, "charlie").await;

    // The player and operator realms do not substitute for each other.
    refusal(
        client
            .post(format!("{base}/v2/console/tournament"))
            .json(&json!({ "id": "anonymous", "duration": 60 }))
            .send()
            .await
            .unwrap(),
        StatusCode::UNAUTHORIZED,
        "define a tournament without a console token",
    )
    .await;
    refusal(
        client
            .get(format!("{base}/v2/tournament"))
            .send()
            .await
            .unwrap(),
        StatusCode::UNAUTHORIZED,
        "browse tournaments without a session",
    )
    .await;

    // Definitions the console must refuse before touching the table.
    for (body, expected) in [
        (json!({ "duration": 60 }), "Tournament ID must be provided."),
        (
            json!({ "id": format!("e2e-bad-duration-{suffix}") }),
            "A tournament needs a duration of at least one second.",
        ),
        (
            json!({
                "id": format!("e2e-bad-size-{suffix}"),
                "duration": 60,
                "max_size": 0
            }),
            "Both max_size and max_num_score must be positive.",
        ),
        (
            json!({
                "id": format!("e2e-bad-schedule-{suffix}"),
                "duration": 60,
                "reset_schedule": "every minute"
            }),
            "leaderboard reset schedule is not a cron expression",
        ),
    ] {
        let message = refusal(
            client
                .post(format!("{base}/v2/console/tournament"))
                .bearer_auth(console)
                .json(&body)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an invalid tournament definition",
        )
        .await;
        assert_eq!(message, expected);
    }

    // Three boards for the player flow and one already closed. Explicit start
    // times make the browser's ordering deterministic.
    let joined = define_tournament(
        client,
        base,
        console,
        json!({
            "id": joined_id.clone(),
            "title": "E2E Joined Tournament",
            "description": "covered by rest_e2e",
            "duration": 3600,
            "start_time": stamp(-300),
            "join_required": true,
            "max_size": 2,
            "max_num_score": 3,
            "metadata": { "lane": "e2e" }
        }),
    )
    .await;
    assert_eq!(joined["id"], joined_id);
    assert_eq!(joined["title"], "E2E Joined Tournament");
    assert_eq!(joined["duration"], 3600);
    assert_eq!(joined["join_required"], true);
    assert_eq!(joined["max_size"], 2);
    assert_eq!(joined["max_num_score"], 3);
    assert_eq!(joined["can_enter"], true);
    assert!(joined["metadata"].as_str().unwrap().contains("lane"));
    assert!(joined["start_time"].is_string());
    assert!(joined.get("size").is_none(), "no entrant prints no size");
    assert!(
        joined.get("end_time").is_none(),
        "an open-ended board has no end time on the wire"
    );

    define_tournament(
        client,
        base,
        console,
        json!({
            "id": open_id.clone(),
            "title": "E2E Open Tournament",
            "duration": 1800,
            "start_time": stamp(-240),
            "max_size": 2,
            "max_num_score": 10
        }),
    )
    .await;
    define_tournament(
        client,
        base,
        console,
        json!({
            "id": authoritative_id.clone(),
            "title": "E2E Authoritative Tournament",
            "duration": 1800,
            "start_time": stamp(-180),
            "authoritative": true
        }),
    )
    .await;
    define_tournament(
        client,
        base,
        console,
        json!({
            "id": ended_id.clone(),
            "title": "E2E Ended Tournament",
            "duration": 3600,
            "start_time": stamp(-7200),
            "end_time": stamp(-3600),
            "join_required": true
        }),
    )
    .await;

    // A plain leaderboard shares the table but is not a tournament: the player
    // routes must neither list it nor address it.
    sqlx::query("INSERT INTO leaderboard (id, title, category, duration) VALUES ($1, $2, 0, 0)")
        .bind(&plain_id)
        .bind("E2E Plain Leaderboard")
        .execute(pool)
        .await
        .expect("seed a plain leaderboard");

    let duplicate = refusal(
        client
            .post(format!("{base}/v2/console/tournament"))
            .bearer_auth(console)
            .json(&json!({ "id": joined_id.clone(), "duration": 60 }))
            .send()
            .await
            .unwrap(),
        StatusCode::CONFLICT,
        "a duplicate tournament id",
    )
    .await;
    assert_eq!(duplicate, "Tournament id is already taken.");

    // The browser lists only tournaments, in start order, and only the ones
    // whose window has not closed.
    let page = json_body(
        client
            .get(format!("{base}/v2/tournament?limit=10"))
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "browse tournaments",
    )
    .await;
    assert_eq!(
        tournament_ids(&page),
        vec![joined_id.clone(), open_id.clone(), authoritative_id.clone()]
    );
    let row = page["tournaments"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == joined_id)
        .expect("the joined tournament is listed");
    assert_eq!(row["can_enter"], true);
    assert_eq!(row["join_required"], true);
    assert!(row.get("size").is_none());

    let first = json_body(
        client
            .get(format!("{base}/v2/tournament?limit=1"))
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "browse tournaments, first page",
    )
    .await;
    assert_eq!(tournament_ids(&first), vec![joined_id.clone()]);
    let cursor = first["cursor"]
        .as_str()
        .expect("a full page carries a cursor")
        .to_owned();
    assert!(!cursor.is_empty());
    let second = json_body(
        client
            .get(format!("{base}/v2/tournament?limit=1&cursor={cursor}"))
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "browse tournaments, second page",
    )
    .await;
    assert_eq!(tournament_ids(&second), vec![open_id.clone()]);

    // The end-time filter is how an operator sees closed rounds; the default
    // listing hides them.
    let closed = json_body(
        client
            .get(format!("{base}/v2/tournament?end_time={}", now.timestamp()))
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "browse ended tournaments",
    )
    .await;
    assert_eq!(tournament_ids(&closed), vec![ended_id.clone()]);

    let empty = json_body(
        client
            .get(format!("{base}/v2/tournament?category_start=2"))
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "browse outside the category",
    )
    .await;
    assert!(tournament_ids(&empty).is_empty());

    for (query, expected) in [
        (
            "start_time=1000&end_time=500",
            "Tournament end time must be greater than start time.",
        ),
        ("limit=101", "Limit must be between 1 and 100."),
        (
            "category_start=5&category_end=1",
            "Tournament category end must be greater than category start.",
        ),
    ] {
        let message = refusal(
            client
                .get(format!("{base}/v2/tournament?{query}"))
                .bearer_auth(&alpha.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an invalid tournament query",
        )
        .await;
        assert_eq!(message, expected);
    }

    for id in ["does-not-exist", plain_id.as_str()] {
        let message = refusal(
            client
                .get(format!("{base}/v2/tournament/{id}"))
                .bearer_auth(&alpha.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "a non-tournament board",
        )
        .await;
        assert_eq!(message, "Tournament not found.");
    }

    // Joining puts a zero-score row in the round and moves the size once.
    let joined_url = format!("{base}/v2/tournament/{joined_id}");
    expect(
        client
            .post(format!("{joined_url}/join"))
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "alpha joins",
    )
    .await;
    let size: i32 = sqlx::query_scalar("SELECT size FROM leaderboard WHERE id = $1")
        .bind(&joined_id)
        .fetch_one(pool)
        .await
        .expect("read the tournament size");
    assert_eq!(size, 1);
    expect(
        client
            .post(format!("{joined_url}/join"))
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "alpha joins twice",
    )
    .await;
    let size: i32 = sqlx::query_scalar("SELECT size FROM leaderboard WHERE id = $1")
        .bind(&joined_id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(size, 1, "a second join is the same entrant");

    let joined_row: (i32, String, i64) = sqlx::query_as(
        "SELECT num_score, username, EXTRACT(EPOCH FROM expiry_time)::bigint
         FROM leaderboard_record WHERE leaderboard_id = $1 AND owner_id = $2",
    )
    .bind(&joined_id)
    .bind(Uuid::parse_str(&alpha.uid).unwrap())
    .fetch_one(pool)
    .await
    .expect("the join row exists");
    assert_eq!(joined_row.0, 0);
    assert_eq!(joined_row.1, alpha.username);
    assert_eq!(
        joined_row.2, 0,
        "an open-ended round stores the epoch window"
    );

    // A join-required board refuses a score from someone who never joined.
    let message = refusal(
        client
            .post(&joined_url)
            .bearer_auth(&bravo.token)
            .json(&json!({ "score": 1 }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "write before joining",
    )
    .await;
    assert_eq!(
        message,
        "Must join tournament before attempting to write value."
    );
    expect(
        client
            .post(format!("{joined_url}/join"))
            .bearer_auth(&bravo.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "bravo joins",
    )
    .await;
    let size: i32 = sqlx::query_scalar("SELECT size FROM leaderboard WHERE id = $1")
        .bind(&joined_id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(size, 2);
    let message = refusal(
        client
            .post(format!("{joined_url}/join"))
            .bearer_auth(&charlie.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "join a full tournament",
    )
    .await;
    assert_eq!(
        message,
        "Tournament cannot be joined as it has reached its max size."
    );

    // Best-score writes, an operator override, and the attempt cap: the last
    // write changes nothing, and no counter moves because the cap is a WHERE.
    let first = write_record(
        client,
        base,
        &alpha.token,
        &joined_id,
        json!({ "score": 10, "metadata": "{\"mood\":\"happy\"}" }),
    )
    .await;
    assert_eq!(first["score"], "10");
    assert_eq!(first["num_score"], 1);
    assert_eq!(first["owner_id"], alpha.uid);
    assert_eq!(first["username"], alpha.username);
    assert_eq!(first["rank"], "1");
    assert!(first["metadata"].as_str().unwrap().contains("happy"));
    assert!(
        first.get("expiry_time").is_none(),
        "the epoch window prints no expiry"
    );

    let second = write_record(
        client,
        base,
        &alpha.token,
        &joined_id,
        json!({ "score": 5 }),
    )
    .await;
    assert_eq!(second["score"], "10", "best keeps the higher score");
    assert_eq!(second["num_score"], 2);

    let third = write_record(
        client,
        base,
        &alpha.token,
        &joined_id,
        json!({ "score": 3, "operator": 2 }),
    )
    .await;
    assert_eq!(third["score"], "3", "operator 2 replaces the score");
    assert_eq!(third["num_score"], 3);

    let capped = write_record(
        client,
        base,
        &alpha.token,
        &joined_id,
        json!({ "score": 4 }),
    )
    .await;
    assert_eq!(capped["score"], "3");
    assert_eq!(
        capped["num_score"], 3,
        "the attempt cap leaves the record untouched"
    );

    let bravo_record = write_record(
        client,
        base,
        &bravo.token,
        &joined_id,
        json!({ "score": 7, "subscore": 4 }),
    )
    .await;
    assert_eq!(bravo_record["score"], "7");
    assert_eq!(bravo_record["subscore"], "4");
    assert_eq!(bravo_record["rank"], "1");
    assert_eq!(bravo_record["num_score"], 1);

    let (stored_score, stored_attempts, stored_username): (i64, i32, String) = sqlx::query_as(
        "SELECT score, num_score, username FROM leaderboard_record
         WHERE leaderboard_id = $1 AND owner_id = $2",
    )
    .bind(&joined_id)
    .bind(Uuid::parse_str(&alpha.uid).unwrap())
    .fetch_one(pool)
    .await
    .expect("alpha's record is stored");
    assert_eq!(stored_score, 3);
    assert_eq!(stored_attempts, 3);
    assert_eq!(stored_username, alpha.username);
    let size: i32 = sqlx::query_scalar("SELECT size FROM leaderboard WHERE id = $1")
        .bind(&joined_id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(size, 2, "writes never move the join counter");

    // The round, ordered by score with ranks, and the named-owner batch.
    let listing = json_body(
        client
            .get(&joined_url)
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "list tournament records",
    )
    .await;
    let records = listing["records"].as_array().expect("records page");
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["owner_id"], bravo.uid);
    assert_eq!(records[0]["score"], "7");
    assert_eq!(records[0]["rank"], "1");
    assert_eq!(records[1]["owner_id"], alpha.uid);
    assert_eq!(records[1]["score"], "3");
    assert_eq!(records[1]["rank"], "2");
    assert_eq!(listing["rank_count"], "2");

    let owners = json_body(
        client
            .get(format!("{joined_url}?owner_ids={}", alpha.uid))
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "list named owner records",
    )
    .await;
    assert_eq!(
        owners["records"].as_array().map(Vec::len),
        Some(2),
        "without a handed-back cursor the round is still paged"
    );
    let own = owners["owner_records"].as_array().expect("owner records");
    assert_eq!(own.len(), 1);
    assert_eq!(own[0]["owner_id"], alpha.uid);
    assert_eq!(own[0]["score"], "3");

    // The tournament route's documented quirk: owners plus a cursor asks for
    // the named records only, so the page is not read at all.
    let paged = json_body(
        client
            .get(format!("{joined_url}?limit=1"))
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "one record of the round",
    )
    .await;
    let cursor = paged["next_cursor"]
        .as_str()
        .expect("a truncated page carries a cursor")
        .to_owned();
    let only_owners = json_body(
        client
            .get(format!(
                "{joined_url}?owner_ids={}&cursor={cursor}",
                alpha.uid
            ))
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "owners with a cursor",
    )
    .await;
    assert!(
        only_owners.get("records").is_none(),
        "owners with a cursor pages nothing"
    );
    let own = only_owners["owner_records"]
        .as_array()
        .expect("owner records");
    assert_eq!(own.len(), 1);
    assert_eq!(own[0]["owner_id"], alpha.uid);

    let around = json_body(
        client
            .get(format!("{joined_url}/owner/{}", alpha.uid))
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "haystack around the owner",
    )
    .await;
    let window = around["records"].as_array().expect("haystack records");
    assert_eq!(window.len(), 2);
    assert!(window.iter().any(|row| row["owner_id"] == alpha.uid));

    for (query, expected) in [
        (
            "limit=101",
            "Invalid limit - limit must be between 1 and 100.",
        ),
        ("owner_ids=nope", "One or more owner IDs are invalid."),
    ] {
        let message = refusal(
            client
                .get(format!("{joined_url}?{query}"))
                .bearer_auth(&alpha.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an invalid records query",
        )
        .await;
        assert_eq!(message, expected);
    }

    // A closed board refuses the round and any write, joins are inactive, and
    // its window reads as empty rather than missing.
    let ended_expectations = [
        (
            client
                .get(format!("{base}/v2/tournament/{ended_id}"))
                .bearer_auth(&alpha.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "Tournament has ended.",
            "list the records of an ended tournament",
        ),
        (
            client
                .post(format!("{base}/v2/tournament/{ended_id}"))
                .bearer_auth(&alpha.token)
                .json(&json!({ "score": 1 }))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "Tournament not found or has ended.",
            "write to an ended tournament",
        ),
        (
            client
                .post(format!("{base}/v2/tournament/{ended_id}/join"))
                .bearer_auth(&alpha.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "Tournament is not active and cannot accept new joins.",
            "join an ended tournament",
        ),
    ];
    for (response, status, expected, what) in ended_expectations {
        let message = refusal(response, status, what).await;
        assert_eq!(message, expected);
    }
    let closed_window = json_body(
        client
            .get(format!(
                "{base}/v2/tournament/{ended_id}/owner/{}",
                alpha.uid
            ))
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "haystack on an ended tournament",
    )
    .await;
    assert!(closed_window.get("records").is_none());

    // An authoritative board only takes server-side submissions.
    let message = refusal(
        client
            .post(format!("{base}/v2/tournament/{authoritative_id}"))
            .bearer_auth(&alpha.token)
            .json(&json!({ "score": 1 }))
            .send()
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "write to an authoritative tournament",
    )
    .await;
    assert_eq!(
        message,
        "Tournament only allows authoritative score submissions."
    );

    // An open board inserts on the first write, refuses a newcomer once full,
    // and releases the place when the row is deleted.
    let open_url = format!("{base}/v2/tournament/{open_id}");
    let open_first = write_record(
        client,
        base,
        &alpha.token,
        &open_id,
        json!({ "score": 100 }),
    )
    .await;
    assert_eq!(open_first["score"], "100");
    assert_eq!(open_first["num_score"], 1);
    let open_second = write_record(
        client,
        base,
        &charlie.token,
        &open_id,
        json!({ "score": 50 }),
    )
    .await;
    assert_eq!(open_second["num_score"], 1);
    let size: i32 = sqlx::query_scalar("SELECT size FROM leaderboard WHERE id = $1")
        .bind(&open_id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(size, 2);

    let better = write_record(client, base, &alpha.token, &open_id, json!({ "score": 90 })).await;
    assert_eq!(better["score"], "100");
    assert_eq!(better["num_score"], 2);
    let incremented = write_record(
        client,
        base,
        &alpha.token,
        &open_id,
        json!({ "score": 5, "operator": 3 }),
    )
    .await;
    assert_eq!(incremented["score"], "105");
    assert_eq!(incremented["num_score"], 3);

    let message = refusal(
        client
            .post(&open_url)
            .bearer_auth(&bravo.token)
            .json(&json!({ "score": 9 }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "write to a full open tournament",
    )
    .await;
    assert_eq!(message, "Tournament has reached max size.");
    let (count, size): (i64, i32) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM leaderboard_record WHERE leaderboard_id = $1),
            (SELECT size FROM leaderboard WHERE id = $1)",
    )
    .bind(&open_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!((count, size), (2, 2), "the refused insert is rolled back");

    expect(
        client
            .delete(&open_url)
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "alpha deletes their record",
    )
    .await;
    let (count, size): (i64, i32) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM leaderboard_record WHERE leaderboard_id = $1),
            (SELECT size FROM leaderboard WHERE id = $1)",
    )
    .bind(&open_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!((count, size), (1, 1));
    let reentered = write_record(client, base, &alpha.token, &open_id, json!({ "score": 7 })).await;
    assert_eq!(reentered["score"], "7");
    assert_eq!(reentered["num_score"], 1);
    let size: i32 = sqlx::query_scalar("SELECT size FROM leaderboard WHERE id = $1")
        .bind(&open_id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(size, 2);
    expect(
        client
            .delete(&open_url)
            .bearer_auth(&bravo.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "a recordless delete is a no-op",
    )
    .await;
    let size: i32 = sqlx::query_scalar("SELECT size FROM leaderboard WHERE id = $1")
        .bind(&open_id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(size, 2, "removing nothing releases nothing");

    // The console edits a tournament in place: omitted fields keep their
    // stored values, and the board cannot be demoted out of the tournament.
    let updated = json_body(
        client
            .put(format!("{base}/v2/console/tournament/{joined_id}"))
            .bearer_auth(console)
            .json(&json!({
                "title": "E2E Joined Tournament (renamed)",
                "max_num_score": 5
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "rename a tournament",
    )
    .await;
    assert_eq!(updated["title"], "E2E Joined Tournament (renamed)");
    assert_eq!(updated["duration"], 3600);
    assert_eq!(updated["max_num_score"], 5);
    let message = refusal(
        client
            .put(format!("{base}/v2/console/tournament/{joined_id}"))
            .bearer_auth(console)
            .json(&json!({ "duration": 0 }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "drop a tournament's duration",
    )
    .await;
    assert_eq!(
        message,
        "A tournament needs a duration of at least one second."
    );
    let message = refusal(
        client
            .put(format!("{base}/v2/console/tournament/{plain_id}"))
            .bearer_auth(console)
            .json(&json!({ "title": "not a tournament" }))
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "update a plain leaderboard as a tournament",
    )
    .await;
    assert_eq!(message, "Tournament not found.");

    // Ending a tournament by hand cascades its records, and the player route
    // stops knowing the id.
    expect(
        client
            .delete(format!("{base}/v2/console/tournament/{ended_id}"))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "delete the ended tournament",
    )
    .await;
    expect(
        client
            .delete(format!("{base}/v2/console/tournament/{joined_id}"))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "delete the joined tournament",
    )
    .await;
    let remaining: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM leaderboard_record WHERE leaderboard_id = $1")
            .bind(&joined_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(remaining, 0, "the board's records cascade with it");
    let message = refusal(
        client
            .get(&joined_url)
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "read a deleted tournament",
    )
    .await;
    assert_eq!(message, "Tournament not found.");
    let message = refusal(
        client
            .delete(format!("{base}/v2/console/tournament/{plain_id}"))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "delete a plain leaderboard as a tournament",
    )
    .await;
    assert_eq!(message, "Tournament not found.");

    for id in [&open_id, &authoritative_id] {
        expect(
            client
                .delete(format!("{base}/v2/console/tournament/{id}"))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::OK,
            "delete a tournament",
        )
        .await;
    }
    let page = json_body(
        client
            .get(format!("{base}/v2/tournament"))
            .bearer_auth(&alpha.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "browse after deletions",
    )
    .await;
    assert!(tournament_ids(&page).is_empty());

    // The plain board is cleaned through its own route.
    expect(
        client
            .delete(format!("{base}/v2/console/leaderboard/{plain_id}"))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "delete the plain leaderboard",
    )
    .await;

    // The console trail records every board transition it performed.
    let (creates, updates, deletes): (i64, i64, i64) = sqlx::query_as(
        "SELECT
            COUNT(*) FILTER (WHERE action = 'tournament.create'),
            COUNT(*) FILTER (WHERE action = 'tournament.update'),
            COUNT(*) FILTER (WHERE action = 'tournament.delete')
         FROM console_audit_log",
    )
    .fetch_one(pool)
    .await
    .expect("read the board audit trail");
    assert_eq!(creates, 4);
    assert_eq!(updates, 1);
    assert_eq!(deletes, 4);
}

// ---------------------------------------------------------------------------
// Player auth, account, session, link/unlink and user lookups
// ---------------------------------------------------------------------------

/// The ten `Authenticate*` routes: the server-key wall, every validation gate,
/// the three providers this build can verify itself (device, custom, email), and
/// the vendor refusals for the other seven.
async fn auth_surface(client: &Client, base: &str, pool: &PgPool) {
    eprintln!("e2e: auth providers");
    let key = server_key_header();
    let url = |route: &str, query: &str| format!("{base}/v2/account/authenticate{route}{query}");

    // The server key is the first wall, and its two refusals differ.
    assert_eq!(
        refusal(
            client
                .post(url("/device", ""))
                .json(&json!({ "id": "e2e-no-key-device" }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "authenticate without a server key",
        )
        .await,
        "Server key required"
    );
    assert_eq!(
        refusal(
            client
                .post(url("/device", ""))
                .header(
                    reqwest::header::AUTHORIZATION,
                    format!(
                        "Basic {}",
                        base64::engine::general_purpose::STANDARD.encode("wrong-key:")
                    ),
                )
                .json(&json!({ "id": "e2e-wrong-key-device" }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "authenticate with the wrong server key",
        )
        .await,
        "Server key invalid"
    );

    // Device: required, character class, then the 10-byte floor.
    for (id, message) in [
        ("", "Device ID is required."),
        ("short", "Device ID invalid, must be 10-128 bytes."),
        (
            "has space in it",
            "Device ID invalid, no spaces or control characters allowed.",
        ),
    ] {
        assert_eq!(
            refusal(
                client
                    .post(url("/device", ""))
                    .header(reqwest::header::AUTHORIZATION, &key)
                    .json(&json!({ "id": id }))
                    .send()
                    .await
                    .unwrap(),
                StatusCode::BAD_REQUEST,
                "device id validation",
            )
            .await,
            message
        );
    }
    let bad_bool = refusal(
        client
            .post(url("/device", "?create=maybe"))
            .header(reqwest::header::AUTHORIZATION, &key)
            .json(&json!({ "id": "e2e-bool-device" }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "device create flag",
    )
    .await;
    assert!(
        bad_bool.contains("cannot parse \"maybe\" as a boolean value"),
        "{bad_bool}"
    );
    assert_eq!(
        refusal(
            client
                .post(url(
                    "/device",
                    &format!("?username={}", "bad[]name".replace(' ', "")),
                ))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "id": "e2e-bad-username-device" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "device username characters",
        )
        .await,
        "Username invalid, no spaces or control characters allowed."
    );
    assert_eq!(
        refusal(
            client
                .post(url("/device", "?username=ab&create=true"))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "id": "e2e-short-name-device" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "device username moderation",
        )
        .await,
        "Username must be 3-20 characters and free of profanity."
    );

    // A device account is created once and resumed afterwards.
    let device_id = format!("e2e-auth-device-{}", Uuid::new_v4());
    let username = format!("authplayer{}", letters_suffix());
    let created = json_body(
        client
            .post(url(
                "/device",
                &format!("?username={username}&create=true"),
            ))
            .header(reqwest::header::AUTHORIZATION, &key)
            .json(&json!({ "id": device_id }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "create a device account",
    )
    .await;
    assert_eq!(created["created"], true);
    let uid = jwt_uid(&created["token"]);
    let resumed = json_body(
        client
            .post(url("/device", "?create=false"))
            .header(reqwest::header::AUTHORIZATION, &key)
            .json(&json!({ "id": device_id }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "resume a device account",
    )
    .await;
    assert!(resumed.get("created").is_none());
    assert_eq!(jwt_uid(&resumed["token"]), uid);
    let stored: String = sqlx::query_scalar("SELECT username FROM users WHERE id = $1")
        .bind(Uuid::parse_str(&uid).unwrap())
        .fetch_one(pool)
        .await
        .expect("the device account row");
    assert_eq!(stored, username);

    // A banned account is refused before anything else, and a provider-linked
    // device row is not a passwordless way back into the account.
    sqlx::query("UPDATE users SET disable_time = now() WHERE id = $1")
        .bind(Uuid::parse_str(&uid).unwrap())
        .execute(pool)
        .await
        .expect("ban the device account");
    assert_eq!(
        refusal(
            client
                .post(url("/device", "?create=false"))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "id": device_id }))
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "resume a banned account",
        )
        .await,
        "User account banned."
    );
    sqlx::query("UPDATE users SET disable_time = '1970-01-01T00:00:00Z' WHERE id = $1")
        .bind(Uuid::parse_str(&uid).unwrap())
        .execute(pool)
        .await
        .expect("unban the device account");
    let provider_device = format!("e2e-provider-device-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO user_device (id, user_id, provider) VALUES ($1, $2, 'steam')")
        .bind(&provider_device)
        .bind(Uuid::parse_str(&uid).unwrap())
        .execute(pool)
        .await
        .expect("write a provider-linked device row");
    assert_eq!(
        refusal(
            client
                .post(url("/device", "?create=false"))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "id": provider_device }))
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "sign in through a provider-linked device row",
        )
        .await,
        "User account uses provider-linked identifier."
    );
    assert_eq!(
        refusal(
            client
                .post(url("/device", "?create=false"))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "id": format!("e2e-missing-device-{}", Uuid::new_v4()) }))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "resume a device that was never created",
        )
        .await,
        "User account not found."
    );

    // Custom ids are a unique column, so the resume path reads `users`.
    for (id, message) in [
        ("", "Custom ID is required."),
        ("short", "Custom ID invalid, must be 6-128 bytes."),
        (
            "has space",
            "Custom ID invalid, no spaces or control characters allowed.",
        ),
    ] {
        assert_eq!(
            refusal(
                client
                    .post(url("/custom", ""))
                    .header(reqwest::header::AUTHORIZATION, &key)
                    .json(&json!({ "id": id }))
                    .send()
                    .await
                    .unwrap(),
                StatusCode::BAD_REQUEST,
                "custom id validation",
            )
            .await,
            message
        );
    }
    let custom_id = format!("custom-{}", letters_suffix());
    let custom_name = format!("custom{}", letters_suffix());
    let custom_created = json_body(
        client
            .post(url(
                "/custom",
                &format!("?username={custom_name}&create=true"),
            ))
            .header(reqwest::header::AUTHORIZATION, &key)
            .json(&json!({ "id": custom_id }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "create a custom-id account",
    )
    .await;
    assert_eq!(custom_created["created"], true);
    let custom_uid = jwt_uid(&custom_created["token"]);
    let custom_resumed = json_body(
        client
            .post(url("/custom", ""))
            .header(reqwest::header::AUTHORIZATION, &key)
            .json(&json!({ "id": custom_id }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "resume a custom-id account",
    )
    .await;
    assert!(custom_resumed.get("created").is_none());
    assert_eq!(jwt_uid(&custom_resumed["token"]), custom_uid);
    assert_eq!(
        refusal(
            client
                .post(url(
                    "/custom",
                    &format!("?username={custom_name}&create=true"),
                ))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "id": format!("other-{}", letters_suffix()) }))
                .send()
                .await
                .unwrap(),
            StatusCode::CONFLICT,
            "create a second account under a taken username",
        )
        .await,
        "Username is already in use."
    );
    assert_eq!(
        refusal(
            client
                .post(url("/custom", "?create=false"))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "id": format!("absent-{}", letters_suffix()) }))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "custom id that was never created",
        )
        .await,
        "User account not found."
    );

    // Email: the only route with a password this server verifies itself.
    assert_eq!(
        refusal(
            client
                .post(url("/email", ""))
                .header(reqwest::header::AUTHORIZATION, &key)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "email authenticate with no body",
        )
        .await,
        "Email address and password is required."
    );
    assert_eq!(
        refusal(
            client
                .post(url("/email", ""))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "email": "nope", "password": "long-enough" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "email address shape",
        )
        .await,
        "Invalid email address format."
    );
    assert_eq!(
        refusal(
            client
                .post(url("/email", ""))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "email": "a@b.co", "password": "long-enough" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "email address length",
        )
        .await,
        "Invalid email address, must be 10-255 bytes."
    );
    assert_eq!(
        refusal(
            client
                .post(url("/email", ""))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "email": "e2e-short-password@example.com", "password": "short" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "email password length",
        )
        .await,
        "Password must be at least 8 characters long."
    );
    let email_name = format!("mail{}", letters_suffix());
    let email = format!("{email_name}@example.com");
    let password = "e2e-password";
    let email_created = json_body(
        client
            .post(url("/email", &format!("?username={email_name}&create=true")))
            .header(reqwest::header::AUTHORIZATION, &key)
            .json(&json!({ "email": email, "password": password }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "create an email account",
    )
    .await;
    assert_eq!(email_created["created"], true);
    let email_uid = jwt_uid(&email_created["token"]);
    let email_login = json_body(
        client
            .post(url("/email", ""))
            .header(reqwest::header::AUTHORIZATION, &key)
            .json(&json!({ "email": email.to_uppercase(), "password": password }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "log in by email (case-folded)",
    )
    .await;
    assert!(email_login.get("created").is_none());
    assert_eq!(jwt_uid(&email_login["token"]), email_uid);
    assert_eq!(
        refusal(
            client
                .post(url("/email", ""))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "email": email, "password": "wrong-password" }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "email login with the wrong password",
        )
        .await,
        "Invalid credentials."
    );
    let username_login = json_body(
        client
            .post(url("/email", &format!("?username={email_name}")))
            .header(reqwest::header::AUTHORIZATION, &key)
            .json(&json!({ "email": "", "password": password }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "log in by username",
    )
    .await;
    assert_eq!(jwt_uid(&username_login["token"]), email_uid);
    assert_eq!(
        refusal(
            client
                .post(url("/email", ""))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "email": "", "password": password }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "username login without a username",
        )
        .await,
        "Username is required when email address is not supplied."
    );
    assert_eq!(
        refusal(
            client
                .post(url("/email", "?create=false"))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "email": format!("absent{}@example.com", letters_suffix()), "password": password }))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "email login that must not create",
        )
        .await,
        "User account not found."
    );
    assert_eq!(
        refusal(
            client
                .post(url("/email", &format!("?username={email_name}&create=true")))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "email": format!("other{}@example.com", letters_suffix()), "password": password }))
                .send()
                .await
                .unwrap(),
            StatusCode::CONFLICT,
            "create a second email account under a taken username",
        )
        .await,
        "Username is already in use."
    );

    // Apple and Steam answer from configuration before a credential is read.
    for provider in ["apple", "steam"] {
        let label = if provider == "apple" { "Apple" } else { "Steam" };
        assert_eq!(
            refusal(
                client
                    .post(url(&format!("/{provider}"), ""))
                    .header(reqwest::header::AUTHORIZATION, &key)
                    .json(&json!({ "token": "vendor-token" }))
                    .send()
                    .await
                    .unwrap(),
                StatusCode::BAD_REQUEST,
                "unconfigured vendor authenticate",
            )
            .await,
            format!("{label} authentication is not configured.")
        );
    }
    for provider in ["facebook", "google"] {
        let label = if provider == "facebook" {
            "Facebook"
        } else {
            "Google"
        };
        assert_eq!(
            refusal(
                client
                    .post(url(&format!("/{provider}"), ""))
                    .header(reqwest::header::AUTHORIZATION, &key)
                    .json(&json!({}))
                    .send()
                    .await
                    .unwrap(),
                StatusCode::BAD_REQUEST,
                "vendor token is required",
            )
            .await,
            format!("{label} access token is required.")
        );
        assert_eq!(
            refusal(
                client
                    .post(url(&format!("/{provider}"), ""))
                    .header(reqwest::header::AUTHORIZATION, &key)
                    .json(&json!({ "token": "vendor-token" }))
                    .send()
                    .await
                    .unwrap(),
                StatusCode::UNAUTHORIZED,
                "vendor token cannot be verified",
            )
            .await,
            format!("Could not authenticate {label} profile.")
        );
    }
    assert_eq!(
        refusal(
            client
                .post(url("/facebookinstantgame", ""))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "instant game signature is required",
        )
        .await,
        "Signed Player Info for a Facebook Instant Game is required."
    );
    assert_eq!(
        refusal(
            client
                .post(url("/facebookinstantgame", ""))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "signedPlayerInfo": "signed" }))
                .send()
                .await
                .unwrap(),
            StatusCode::INTERNAL_SERVER_ERROR,
            "instant game signature cannot be verified",
        )
        .await,
        "Error extracting the Facebook Instant Game player ID or validating the signature"
    );
    assert_eq!(
        refusal(
            client
                .post(url("/gamecenter", ""))
                .header(reqwest::header::AUTHORIZATION, &key)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "gamecenter credentials are required",
        )
        .await,
        "GameCenter access credentials are required."
    );
    assert_eq!(
        refusal(
            client
                .post(url("/gamecenter", ""))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "bundle_id": "com.example.e2e" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "gamecenter fields are checked in order",
        )
        .await,
        "GameCenter player ID is required."
    );
    assert_eq!(
        refusal(
            client
                .post(url("/gamecenter", ""))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({
                    "bundle_id": "com.example.e2e",
                    "player_id": "G:12345",
                    "timestamp_seconds": 1_700_000_000i64,
                    "salt": "salt",
                    "signature": "signature",
                    "public_key_url": "https://example.test/key",
                }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "gamecenter signature cannot be verified",
        )
        .await,
        "Could not authenticate GameCenter profile."
    );

    // The runtime-provider registry is empty in this build.
    assert_eq!(
        refusal(
            client
                .post(url("", ""))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "runtime provider name is required",
        )
        .await,
        "Provider name is required."
    );
    assert_eq!(
        refusal(
            client
                .post(url("", ""))
                .header(reqwest::header::AUTHORIZATION, &key)
                .json(&json!({ "provider": "GitHub" }))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "unknown runtime provider",
        )
        .await,
        "Authentication provider not found: github"
    );
}

/// `GET`/`PUT`/`DELETE /v2/account`, the wallet routes, and the session pair.
async fn account_surface(client: &Client, base: &str, pool: &PgPool) {
    eprintln!("e2e: account, wallet and session");
    let player = device_player(client, base, "acct").await;
    let other = device_player(client, base, "acctb").await;

    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/account"))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "account without a token",
        )
        .await,
        "Auth token required"
    );
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/account"))
                .bearer_auth("not-a-jwt")
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "account with an unparsable token",
        )
        .await,
        "Auth token invalid"
    );

    let account = json_body(
        client
            .get(format!("{base}/v2/account"))
            .bearer_auth(&player.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "read the account",
    )
    .await;
    assert_eq!(account["user_id"], player.uid);
    assert_eq!(account["username"], player.username);
    assert_eq!(account["lang_tag"], "en");
    assert_eq!(account["metadata"], json!({}));
    assert_eq!(account["wallet"], json!({}));
    assert_eq!(account["edge_count"], 0);
    assert!(account["display_name"].is_null());
    assert!(account["create_time"].as_str().unwrap().ends_with('Z'));
    let devices = account["device_accounts"].as_array().unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0]["id"], player.device_id);
    assert_eq!(devices[0]["provider"], "");
    assert!(devices[0]["vars"]["push_token_ios"].is_string());

    let renamed = format!("renamed{}", letters_suffix());
    let updated = json_body(
        client
            .put(format!("{base}/v2/account"))
            .bearer_auth(&player.token)
            .json(&json!({
                "display_name": "E2E Hero",
                "avatar_url": "https://example.test/hero.png",
                "lang_tag": "fr",
                "location": "Test Range",
                "timezone": "UTC",
                "metadata": "{\"tier\":\"gold\"}",
                "username": renamed,
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "update the account",
    )
    .await;
    assert_eq!(updated["username"], renamed);
    assert_eq!(updated["display_name"], "E2E Hero");
    assert_eq!(updated["avatar_url"], "https://example.test/hero.png");
    assert_eq!(updated["lang_tag"], "fr");
    assert_eq!(updated["location"], "Test Range");
    assert_eq!(updated["timezone"], "UTC");
    assert_eq!(updated["metadata"], json!({ "tier": "gold" }));
    assert_eq!(
        refusal(
            client
                .put(format!("{base}/v2/account"))
                .bearer_auth(&player.token)
                .json(&json!({ "metadata": "{broken" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "account metadata must parse",
        )
        .await,
        "invalid account metadata"
    );

    // A custom id moves between accounts only through unlink, so a second claim is a conflict.
    let shared_custom = format!("shared-{}", letters_suffix());
    let claimed = json_body(
        client
            .put(format!("{base}/v2/account"))
            .bearer_auth(&player.token)
            .json(&json!({ "custom_id": shared_custom }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "claim a custom id",
    )
    .await;
    assert_eq!(claimed["custom_id"], shared_custom);
    assert_eq!(
        refusal(
            client
                .put(format!("{base}/v2/account"))
                .bearer_auth(&other.token)
                .json(&json!({ "custom_id": shared_custom }))
                .send()
                .await
                .unwrap(),
            StatusCode::CONFLICT,
            "claim a taken custom id",
        )
        .await,
        "custom id already in use by another account"
    );

    // The wallet is read-only for players: currency keys are the console's.
    let wallet = json_body(
        client
            .get(format!("{base}/v2/account/wallet"))
            .bearer_auth(&player.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "read the wallet",
    )
    .await;
    assert_eq!(wallet["wallet"], json!({}));
    assert!(wallet["ledger_items"].as_array().unwrap().is_empty());
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/wallet"))
                .bearer_auth(&player.token)
                .json(&json!({ "wallet_items": { "gold": 10 } }))
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "a player cannot mint currency",
        )
        .await,
        "Currency changes require a console write; a player may only merge ledger metadata."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/wallet"))
                .bearer_auth(&player.token)
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an empty wallet write",
        )
        .await,
        "There is nothing to write: send ledger metadata to merge."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/wallet"))
                .bearer_auth(&player.token)
                .json(&json!({ "ledger_items": [{ "id": "not-a-uuid", "metadata": {} }] }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a malformed ledger id",
        )
        .await,
        "Invalid ledger item id."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/wallet"))
                .bearer_auth(&player.token)
                .json(&json!({ "ledger_items": [{ "id": Uuid::new_v4().to_string(), "metadata": {} }] }))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "a ledger entry that does not exist",
        )
        .await,
        "That ledger entry does not belong to this account."
    );

    let player_uid = Uuid::parse_str(&player.uid).unwrap();
    let other_uid = Uuid::parse_str(&other.uid).unwrap();
    let ledger_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO wallet_ledger (id, user_id, changeset, metadata) \
         VALUES ($1, $2, '{\"gold\":25}'::jsonb, '{}'::jsonb)",
    )
    .bind(ledger_id)
    .bind(player_uid)
    .execute(pool)
    .await
    .expect("seed a ledger entry");
    let merged = json_body(
        client
            .post(format!("{base}/v2/account/wallet"))
            .bearer_auth(&player.token)
            .json(&json!({ "ledger_items": [{ "id": ledger_id.to_string(), "metadata": { "note": "merged" } }] }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "merge ledger metadata",
    )
    .await;
    let row = merged["ledger_items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == ledger_id.to_string())
        .expect("the merged ledger row is returned");
    assert_eq!(row["metadata"]["note"], "merged");
    // Merging metadata never moves the balance.
    assert_eq!(merged["wallet"], json!({}));

    let second_ledger = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO wallet_ledger (id, user_id, changeset, metadata) \
         VALUES ($1, $2, '{\"gems\":5}'::jsonb, '{}'::jsonb)",
    )
    .bind(second_ledger)
    .bind(player_uid)
    .execute(pool)
    .await
    .expect("seed a second ledger entry");
    json_body(
        client
            .put(format!("{base}/v2/account/wallet"))
            .bearer_auth(&player.token)
            .json(&json!({ "ledger_items": [{ "id": second_ledger.to_string(), "metadata": { "note": "put" } }] }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the PUT wallet variant merges too",
    )
    .await;
    let foreign_ledger = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO wallet_ledger (id, user_id, changeset, metadata) \
         VALUES ($1, $2, '{}'::jsonb, '{}'::jsonb)",
    )
    .bind(foreign_ledger)
    .bind(other_uid)
    .execute(pool)
    .await
    .expect("seed another account's ledger entry");
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/wallet"))
                .bearer_auth(&player.token)
                .json(&json!({ "ledger_items": [{ "id": foreign_ledger.to_string(), "metadata": {} }] }))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "merge another account's ledger entry",
        )
        .await,
        "That ledger entry does not belong to this account."
    );

    // Refresh: server key in, a new pair under the same family out.
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/session/refresh"))
                .json(&json!({ "token": player.refresh_token }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "refresh without the server key",
        )
        .await,
        "Server key required"
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/session/refresh"))
                .header(reqwest::header::AUTHORIZATION, server_key_header())
                .json(&json!({ "token": "" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "refresh without a token",
        )
        .await,
        "Refresh token is required."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/session/refresh"))
                .header(reqwest::header::AUTHORIZATION, server_key_header())
                .json(&json!({ "token": "not-a-token" }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "refresh with an unparsable token",
        )
        .await,
        "Refresh token invalid or expired."
    );
    let refreshed = json_body(
        client
            .post(format!("{base}/v2/account/session/refresh"))
            .header(reqwest::header::AUTHORIZATION, server_key_header())
            .json(&json!({ "token": player.refresh_token, "vars": { "platform": "refreshed" } }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "refresh the session",
    )
    .await;
    assert!(refreshed.get("created").is_none());
    assert_eq!(jwt_uid(&refreshed["token"]), player.uid);
    let refreshed_token = refreshed["token"].as_str().unwrap().to_owned();
    let refreshed_refresh = refreshed["refresh_token"].as_str().unwrap().to_owned();
    // The freshly minted access token is live.
    json_body(
        client
            .get(format!("{base}/v2/account"))
            .bearer_auth(&refreshed_token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "read the account with the refreshed token",
    )
    .await;

    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/session/logout"))
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "logout without a token",
        )
        .await,
        "Auth token required"
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/session/logout"))
                .bearer_auth(&refreshed_token)
                .json(&json!({ "token": "not-a-token" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "logout naming an unparsable token",
        )
        .await,
        "Session token invalid."
    );
    let logged_out = json_body(
        client
            .post(format!("{base}/v2/session/logout"))
            .bearer_auth(&refreshed_token)
            .json(&json!({ "token": refreshed_token, "refresh_token": refreshed_refresh }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "log out the pair",
    )
    .await;
    assert_eq!(logged_out, json!({}));
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/account"))
                .bearer_auth(&refreshed_token)
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "a logged-out token is revoked",
        )
        .await,
        "Auth token invalid"
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/session/refresh"))
                .header(reqwest::header::AUTHORIZATION, server_key_header())
                .json(&json!({ "token": refreshed_refresh }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "a logged-out refresh token is revoked",
        )
        .await,
        "Refresh token invalid or expired."
    );

    // Deleting an account writes the tombstone and removes the row.
    let doomed = device_player(client, base, "del").await;
    let deleted = json_body(
        client
            .delete(format!("{base}/v2/account"))
            .bearer_auth(&doomed.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "delete the account",
    )
    .await;
    assert_eq!(deleted, json!({}));
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/account"))
                .bearer_auth(&doomed.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "read a deleted account",
        )
        .await,
        "Entity not found."
    );
    let doomed_uid = Uuid::parse_str(&doomed.uid).unwrap();
    let tombstoned: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_tombstone WHERE user_id = $1")
        .bind(doomed_uid)
        .fetch_one(pool)
        .await
        .expect("count the tombstone");
    assert_eq!(tombstoned, 1);
    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE id = $1")
        .bind(doomed_uid)
        .fetch_one(pool)
        .await
        .expect("count the deleted row");
    assert_eq!(remaining, 0);
}

/// `/v2/account/link*` and `/v2/account/unlink*`: the three self-verifiable
/// credentials attach and detach, the vendor ones refuse after their field
/// checks, and no unlink may leave the account with no way back in.
async fn link_surface(client: &Client, base: &str, pool: &PgPool) {
    eprintln!("e2e: link and unlink");
    let a = device_player(client, base, "linka").await;
    let b = device_player(client, base, "linkb").await;

    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/link/device"))
                .json(&json!({ "id": "e2e-anonymous-device" }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "link without a token",
        )
        .await,
        "Auth token required"
    );
    for (route, body, message) in [
        (
            "device",
            json!({}),
            "Device ID is required.",
        ),
        (
            "device",
            json!({ "id": "short" }),
            "Device ID invalid, must be 10-128 bytes.",
        ),
        (
            "custom",
            json!({}),
            "Custom ID is required.",
        ),
        (
            "custom",
            json!({ "id": "short" }),
            "Invalid custom ID, must be 6-128 bytes.",
        ),
        (
            "custom",
            json!({ "id": "has space" }),
            "Invalid custom ID, no spaces or control characters allowed.",
        ),
        (
            "email",
            json!({}),
            "Email address and password is required.",
        ),
        (
            "email",
            json!({ "email": "e2e-link@example.com", "password": "short" }),
            "Password must be at least 8 characters long.",
        ),
        (
            "email",
            json!({ "email": "nope", "password": "long-enough" }),
            "Invalid email address format.",
        ),
    ] {
        assert_eq!(
            refusal(
                client
                    .post(format!("{base}/v2/account/link/{route}"))
                    .bearer_auth(&a.token)
                    .json(&body)
                    .send()
                    .await
                    .unwrap(),
                StatusCode::BAD_REQUEST,
                "link validation",
            )
            .await,
            message
        );
    }

    // B has one device and nothing else, so its only unlink is the guarded one.
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/unlink/device"))
                .bearer_auth(&b.token)
                .json(&json!({ "id": b.device_id }))
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "unlink B's last identifier",
        )
        .await,
        "Cannot unlink last account identifier. Check profile exists and is not last link."
    );

    // Custom id, a second device and an email all attach to A.
    let custom = format!("link-{}", letters_suffix());
    json_body(
        client
            .post(format!("{base}/v2/account/link/custom"))
            .bearer_auth(&a.token)
            .json(&json!({ "id": custom }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "link a custom id",
    )
    .await;
    let b_custom = format!("blink-{}", letters_suffix());
    json_body(
        client
            .post(format!("{base}/v2/account/link/custom"))
            .bearer_auth(&b.token)
            .json(&json!({ "id": b_custom }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "link B's custom id",
    )
    .await;
    let second_device = format!("e2e-second-device-{}", Uuid::new_v4());
    json_body(
        client
            .post(format!("{base}/v2/account/link/device"))
            .bearer_auth(&a.token)
            .json(&json!({ "id": second_device }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "link a second device",
    )
    .await;
    let email = format!("link{}@example.com", letters_suffix());
    json_body(
        client
            .post(format!("{base}/v2/account/link/email"))
            .bearer_auth(&a.token)
            .json(&json!({ "email": email, "password": "e2e-password" }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "link an email",
    )
    .await;

    // A link never moves an identity: every one of those identifiers is now taken.
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/link/custom"))
                .bearer_auth(&b.token)
                .json(&json!({ "id": custom }))
                .send()
                .await
                .unwrap(),
            StatusCode::CONFLICT,
            "claim a custom id already linked",
        )
        .await,
        "Custom ID is already in use."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/link/device"))
                .bearer_auth(&b.token)
                .json(&json!({ "id": second_device }))
                .send()
                .await
                .unwrap(),
            StatusCode::CONFLICT,
            "claim a device already linked",
        )
        .await,
        "Device ID already in use."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/link/email"))
                .bearer_auth(&b.token)
                .json(&json!({ "email": email, "password": "e2e-password" }))
                .send()
                .await
                .unwrap(),
            StatusCode::CONFLICT,
            "claim an email already linked",
        )
        .await,
        "Email is already in use."
    );

    let account = json_body(
        client
            .get(format!("{base}/v2/account"))
            .bearer_auth(&a.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "read the linked account",
    )
    .await;
    assert_eq!(account["custom_id"], custom);
    assert_eq!(account["email"], email.to_lowercase());
    let devices = account["device_accounts"].as_array().unwrap();
    assert_eq!(devices.len(), 2);
    assert!(devices.iter().any(|device| device["id"] == second_device));

    // The seven vendor links stop at their verification step.
    for (route, body, status, message) in [
        (
            "apple",
            json!({ "token": "vendor-token" }),
            StatusCode::BAD_REQUEST,
            "Apple authentication is not configured.",
        ),
        (
            "steam",
            json!({}),
            StatusCode::BAD_REQUEST,
            "Steam access token is required.",
        ),
        (
            "steam",
            json!({ "account": { "token": "vendor-token" } }),
            StatusCode::BAD_REQUEST,
            "Steam authentication is not configured.",
        ),
        (
            "facebook",
            json!({}),
            StatusCode::BAD_REQUEST,
            "Facebook access token is required.",
        ),
        (
            "facebook",
            json!({ "token": "vendor-token" }),
            StatusCode::UNAUTHORIZED,
            "Could not authenticate Facebook profile.",
        ),
        (
            "google",
            json!({ "token": "vendor-token" }),
            StatusCode::UNAUTHORIZED,
            "Could not authenticate Google profile.",
        ),
        (
            "facebookinstantgame",
            json!({}),
            StatusCode::BAD_REQUEST,
            "Signed Player Info for a Facebook Instant Game is required.",
        ),
        (
            "facebookinstantgame",
            json!({ "signed_player_info": "signed" }),
            StatusCode::UNAUTHORIZED,
            "Could not authenticate Facebook Instant Game profile.",
        ),
        (
            "gamecenter",
            json!({ "bundle_id": "com.example.e2e" }),
            StatusCode::BAD_REQUEST,
            "GameCenter player ID is required.",
        ),
    ] {
        assert_eq!(
            refusal(
                client
                    .post(format!("{base}/v2/account/link/{route}"))
                    .bearer_auth(&a.token)
                    .json(&body)
                    .send()
                    .await
                    .unwrap(),
                status,
                "vendor link",
            )
            .await,
            message
        );
    }
    let bad_sync = refusal(
        client
            .post(format!("{base}/v2/account/link/facebook?sync=maybe"))
            .bearer_auth(&a.token)
            .json(&json!({ "token": "vendor-token" }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "facebook sync flag",
    )
    .await;
    assert!(bad_sync.contains("cannot parse \"maybe\" as a boolean value"));
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/link"))
                .bearer_auth(&a.token)
                .json(&json!({ "provider": "GitHub" }))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "runtime provider link",
        )
        .await,
        "Authentication provider not found: github"
    );

    // The guard: no unlink may leave the account with nothing to sign in with.
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/unlink/device"))
                .bearer_auth(&a.token)
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "unlink without a device id",
        )
        .await,
        "A device ID must be supplied."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/unlink/device"))
                .bearer_auth(&a.token)
                .json(&json!({ "id": format!("e2e-not-owned-{}", Uuid::new_v4()) }))
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "unlink a device the account does not own",
        )
        .await,
        "Cannot unlink last account identifier. Check profile exists and is not last link."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/unlink/custom"))
                .bearer_auth(&a.token)
                .json(&json!({ "id": format!("wrong-{}", letters_suffix()) }))
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "unlink with the wrong custom id",
        )
        .await,
        "Cannot unlink last account identifier. Check profile exists and is not last link."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/unlink/email"))
                .bearer_auth(&a.token)
                .json(&json!({ "email": "wrong@example.com" }))
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "unlink with the wrong email",
        )
        .await,
        "Cannot unlink last account identifier. Check profile exists and is not last link."
    );

    // The correct values detach; B loses its custom id first, then its device.
    json_body(
        client
            .post(format!("{base}/v2/account/unlink/custom"))
            .bearer_auth(&b.token)
            .json(&json!({ "id": b_custom }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "unlink B's custom id",
    )
    .await;
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/unlink/device"))
                .bearer_auth(&b.token)
                .json(&json!({ "id": b.device_id }))
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "B's device is the last identifier again",
        )
        .await,
        "Cannot unlink last account identifier. Check profile exists and is not last link."
    );
    json_body(
        client
            .post(format!("{base}/v2/account/unlink/custom"))
            .bearer_auth(&a.token)
            .json(&json!({ "id": custom }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "unlink A's custom id",
    )
    .await;
    json_body(
        client
            .post(format!("{base}/v2/account/unlink/email"))
            .bearer_auth(&a.token)
            .json(&json!({}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "unlink A's email",
    )
    .await;
    let unlinked = json_body(
        client
            .get(format!("{base}/v2/account"))
            .bearer_auth(&a.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "read the unlinked account",
    )
    .await;
    assert!(unlinked["custom_id"].is_null());
    assert!(unlinked["email"].is_null());
    let stored_password: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT password FROM users WHERE id = $1")
            .bind(Uuid::parse_str(&a.uid).unwrap())
            .fetch_one(pool)
            .await
            .expect("read the password column");
    assert!(stored_password.is_none(), "unlinking an email drops the password");

    // The vendor unlinks clear a column on an empty credential and refuse a non-empty one.
    for (route, body, status, message) in [
        (
            "apple",
            json!({ "token": "vendor-token" }),
            StatusCode::UNAUTHORIZED,
            "Could not authenticate Apple profile.",
        ),
        (
            "facebook",
            json!({ "token": "vendor-token" }),
            StatusCode::UNAUTHORIZED,
            "Could not authenticate Facebook profile.",
        ),
        (
            "google",
            json!({ "token": "vendor-token" }),
            StatusCode::UNAUTHORIZED,
            "Could not authenticate Google profile.",
        ),
        (
            "steam",
            json!({ "token": "vendor-token" }),
            StatusCode::UNAUTHORIZED,
            "Could not authenticate Steam profile.",
        ),
        (
            "facebookinstantgame",
            json!({ "signed_player_info": "signed" }),
            StatusCode::UNAUTHORIZED,
            "Could not authenticate Facebook Instant Game profile.",
        ),
    ] {
        assert_eq!(
            refusal(
                client
                    .post(format!("{base}/v2/account/unlink/{route}"))
                    .bearer_auth(&a.token)
                    .json(&body)
                    .send()
                    .await
                    .unwrap(),
                status,
                "vendor unlink with a credential",
            )
            .await,
            message
        );
        json_body(
            client
                .post(format!("{base}/v2/account/unlink/{route}"))
                .bearer_auth(&a.token)
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::OK,
            "vendor unlink without a credential",
        )
        .await;
    }
    let partial_gamecenter = json!({ "bundle_id": "com.example.e2e" });
    json_body(
        client
            .post(format!("{base}/v2/account/unlink/gamecenter"))
            .bearer_auth(&a.token)
            .json(&partial_gamecenter)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "a partial gamecenter bundle is not a refinement",
    )
    .await;
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/unlink/gamecenter"))
                .bearer_auth(&a.token)
                .json(&json!({
                    "bundle_id": "com.example.e2e",
                    "player_id": "G:12345",
                    "timestamp_seconds": 1_700_000_000i64,
                    "salt": "salt",
                    "signature": "signature",
                    "public_key_url": "https://example.test/key",
                }))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "a complete gamecenter bundle must be verified",
        )
        .await,
        "Could not authenticate GameCenter profile."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/unlink"))
                .bearer_auth(&a.token)
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "unlink without a provider name",
        )
        .await,
        "A provider name must be supplied."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/unlink"))
                .bearer_auth(&a.token)
                .json(&json!({ "provider": "steam" }))
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "unlink a provider with no rows",
        )
        .await,
        "Cannot unlink last account identifier. Check profile exists and is not last link."
    );

    // A now holds two device rows and nothing else; removing the second is fine,
    // removing the first is the guard.
    json_body(
        client
            .post(format!("{base}/v2/account/unlink/device"))
            .bearer_auth(&a.token)
            .json(&json!({ "id": second_device }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "unlink the second device",
    )
    .await;
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/account/unlink/device"))
                .bearer_auth(&a.token)
                .json(&json!({ "id": a.device_id }))
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "the last device cannot go",
        )
        .await,
        "Cannot unlink last account identifier. Check profile exists and is not last link."
    );
}

/// `GET`/`POST /v2/user` and `GET /v2/user/{id}`: the public view, its
/// miss-is-absent semantics, and the disabled-account hiding rule.
async fn user_surface(client: &Client, base: &str, pool: &PgPool) {
    eprintln!("e2e: user lookups");
    let a = device_player(client, base, "usr").await;
    let b = device_player(client, base, "usrb").await;

    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/user?ids={}", a.uid))
                .send()
                .await
                .unwrap(),
            StatusCode::UNAUTHORIZED,
            "user lookup without a token",
        )
        .await,
        "Auth token required"
    );

    let empty = json_body(
        client
            .get(format!("{base}/v2/user"))
            .bearer_auth(&a.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "user lookup with nothing requested",
    )
    .await;
    assert!(empty["users"].as_array().unwrap().is_empty());

    let found = json_body(
        client
            .get(format!("{base}/v2/user?ids={}", a.uid))
            .bearer_auth(&a.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "user lookup by id",
    )
    .await;
    let users = found["users"].as_array().unwrap();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0]["id"], a.uid);
    assert_eq!(users[0]["username"], a.username);
    assert_eq!(users[0]["metadata"], "{}");
    assert_eq!(users[0]["lang_tag"], "en");
    assert!(users[0]["edge_count"].is_null());
    assert!(users[0]["create_time"].as_str().unwrap().ends_with('Z'));

    let both = json_body(
        client
            .get(format!(
                "{base}/v2/user?ids={},{}&usernames={}",
                a.uid, b.uid, b.username
            ))
            .bearer_auth(&a.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "user lookup by comma-joined keys",
    )
    .await;
    assert_eq!(both["users"].as_array().unwrap().len(), 3);

    let missing = json_body(
        client
            .get(format!(
                "{base}/v2/user?ids={}&usernames=absent{}",
                Uuid::new_v4(),
                letters_suffix()
            ))
            .bearer_auth(&a.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "user lookup with misses",
    )
    .await;
    assert!(missing["users"].as_array().unwrap().is_empty());

    let bad_id = refusal(
        client
            .get(format!("{base}/v2/user?ids=not-a-uuid"))
            .bearer_auth(&a.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "user lookup with a malformed id",
    )
    .await;
    assert_eq!(bad_id, "ID 'not-a-uuid' is not a valid system ID.");

    let by_body = json_body(
        client
            .post(format!("{base}/v2/user"))
            .bearer_auth(&a.token)
            .json(&json!({ "ids": [a.uid], "usernames": [b.username] }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "user multi-get by body",
    )
    .await;
    assert_eq!(by_body["users"].as_array().unwrap().len(), 2);

    let single = json_body(
        client
            .get(format!("{base}/v2/user/{}", a.uid))
            .bearer_auth(&a.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "get a user by id",
    )
    .await;
    assert_eq!(single["users"][0]["username"], a.username);
    let by_name = json_body(
        client
            .get(format!("{base}/v2/user/{}", b.username))
            .bearer_auth(&a.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "get a user by username",
    )
    .await;
    assert_eq!(by_name["users"][0]["id"], b.uid);
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/user/absent{}", letters_suffix()))
                .bearer_auth(&a.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "get a user that does not exist",
        )
        .await,
        "User account not found."
    );

    // A disabled account has no public view.
    sqlx::query("UPDATE users SET disable_time = now() WHERE id = $1")
        .bind(Uuid::parse_str(&b.uid).unwrap())
        .execute(pool)
        .await
        .expect("disable the second user");
    let hidden = json_body(
        client
            .get(format!("{base}/v2/user?ids={}", b.uid))
            .bearer_auth(&a.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "a disabled account is absent from the multi-get",
    )
    .await;
    assert!(hidden["users"].as_array().unwrap().is_empty());
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/user/{}", b.uid))
                .bearer_auth(&a.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "a disabled account has no single-get",
        )
        .await,
        "User account not found."
    );
    sqlx::query("UPDATE users SET disable_time = '1970-01-01T00:00:00Z' WHERE id = $1")
        .bind(Uuid::parse_str(&b.uid).unwrap())
        .execute(pool)
        .await
        .expect("re-enable the second user");
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
