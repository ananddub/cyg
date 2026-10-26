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

/// An 8-character random lowercase suffix for e2e identifiers.
///
/// Letters only: the moderation filter is a context matcher that reads digit
/// runs as leetspeak, so identifiers that pass through it must not sample
/// digits. `device_player` additionally re-checks the assembled username,
/// because the boundary between prefix and suffix can itself form a match.
fn letters_suffix() -> String {
    let mut value = Uuid::new_v4().as_u128();
    let mut out = String::new();
    while out.len() < 8 {
        out.push((b'a' + (value % 26) as u8) as char);
        value /= 26;
    }
    out
}

/// The same rustrict call `moderation::is_clean` makes on the server.
fn accepts_username(candidate: &str) -> bool {
    use rustrict::{Censor, Type};
    !Censor::from_str(candidate)
        .with_censor_threshold(Type::INAPPROPRIATE)
        .analyze()
        .is(Type::INAPPROPRIATE)
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
    friend_surface(&client, &base, &env.pool).await;
    group_surface(&client, &base, &env.pool).await;
    channel_surface(&client, &base, &env.pool).await;
    notification_surface(&client, &base, &env.pool).await;
    storage_surface(&client, &base).await;
    storage_index_surface(&client, &base).await;
    inventory_surface(&client, &base).await;
    leaderboard_surface(&client, &base, &env.pool).await;
    iap_surface(&client, &base, &env.pool).await;
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
    let username = loop {
        let candidate = format!("e2e-{label}-{}", letters_suffix());
        if accepts_username(&candidate) {
            break candidate;
        }
    };
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

// ---------------------------------------------------------------------------
// Helper verbs for the social, storage and commerce planes
// ---------------------------------------------------------------------------

/// A GET as a player session, asserting 200 and decoding the JSON answer.
async fn api_get(client: &Client, url: String, token: &str, what: &str) -> Value {
    json_body(
        client
            .get(url)
            .bearer_auth(token)
            .send()
            .await
            .unwrap_or_else(|err| panic!("{what}: {err}")),
        StatusCode::OK,
        what,
    )
    .await
}

async fn api_post(client: &Client, url: String, token: &str, body: Value, what: &str) -> Value {
    json_body(
        client
            .post(url)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .unwrap_or_else(|err| panic!("{what}: {err}")),
        StatusCode::OK,
        what,
    )
    .await
}

/// A verb whose inputs are all in the query string, with no body.
async fn api_post_empty(client: &Client, url: String, token: &str, what: &str) -> Value {
    json_body(
        client
            .post(url)
            .bearer_auth(token)
            .send()
            .await
            .unwrap_or_else(|err| panic!("{what}: {err}")),
        StatusCode::OK,
        what,
    )
    .await
}

async fn api_put(client: &Client, url: String, token: &str, body: Value, what: &str) -> Value {
    json_body(
        client
            .put(url)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .unwrap_or_else(|err| panic!("{what}: {err}")),
        StatusCode::OK,
        what,
    )
    .await
}

async fn api_delete(client: &Client, url: String, token: &str, what: &str) -> Value {
    json_body(
        client
            .delete(url)
            .bearer_auth(token)
            .send()
            .await
            .unwrap_or_else(|err| panic!("{what}: {err}")),
        StatusCode::OK,
        what,
    )
    .await
}

async fn api_delete_body(
    client: &Client,
    url: String,
    token: &str,
    body: Value,
    what: &str,
) -> Value {
    json_body(
        client
            .delete(url)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .unwrap_or_else(|err| panic!("{what}: {err}")),
        StatusCode::OK,
        what,
    )
    .await
}

/// Build a URL with query parameters percent-encoded by the same codec the server
/// decodes with, so JSON-array values and duplicates travel honestly.
fn with_query(base: &str, pairs: &[(&str, &str)]) -> String {
    let mut url = reqwest::Url::parse(base).expect("a valid base URL");
    {
        let mut query = url.query_pairs_mut();
        for (key, value) in pairs {
            query.append_pair(key, value);
        }
    }
    url.to_string()
}

fn uuid_of(player: &Player) -> Uuid {
    Uuid::parse_str(&player.uid).expect("the session's uid claim is a UUID")
}

/// The gateway carries metadata, notification content and stored values as JSON
/// strings on the wire; this unwraps one back into a value for comparison.
fn json_string(value: &Value) -> Value {
    serde_json::from_str(value.as_str().expect("a JSON string member"))
        .unwrap_or_else(|err| panic!("member is not JSON ({err}): {value}"))
}

async fn edge_count(pool: &PgPool, uid: Uuid) -> i32 {
    sqlx::query_scalar("SELECT edge_count FROM users WHERE id = $1")
        .bind(uid)
        .fetch_one(pool)
        .await
        .expect("users.edge_count")
}

async fn friend_edge_state(pool: &PgPool, from: Uuid, to: Uuid) -> Option<i16> {
    sqlx::query_scalar("SELECT state FROM user_edge WHERE source_id = $1 AND destination_id = $2")
        .bind(from)
        .bind(to)
        .fetch_optional(pool)
        .await
        .expect("user_edge state")
}

async fn group_edge_state(pool: &PgPool, uid: Uuid, group: Uuid) -> Option<i16> {
    sqlx::query_scalar("SELECT state FROM group_edge WHERE source_id = $1 AND destination_id = $2")
        .bind(uid)
        .bind(group)
        .fetch_optional(pool)
        .await
        .expect("group_edge state")
}

async fn group_count(pool: &PgPool, group: Uuid) -> i32 {
    sqlx::query_scalar("SELECT edge_count FROM groups WHERE id = $1")
        .bind(group)
        .fetch_one(pool)
        .await
        .expect("groups.edge_count")
}

// ---------------------------------------------------------------------------
// Friends
// ---------------------------------------------------------------------------

async fn friend_surface(client: &Client, base: &str, pool: &PgPool) {
    eprintln!("e2e: friends");
    let alice = device_player(client, base, "fr-a").await;
    let bob = device_player(client, base, "fr-b").await;
    let carol = device_player(client, base, "fr-c").await;
    let (a, b, c) = (uuid_of(&alice), uuid_of(&bob), uuid_of(&carol));

    let empty = api_get(
        client,
        format!("{base}/v2/friend"),
        &alice.token,
        "an empty friend list",
    )
    .await;
    assert_eq!(empty["friends"], json!([]));
    assert_eq!(empty["cursor"], "");

    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/friend?limit=0"))
                .bearer_auth(&alice.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "friend list limit below one",
        )
        .await,
        "Invalid limit - limit must be between 1 and 1000."
    );
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/friend?state=4"))
                .bearer_auth(&alice.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "friend list state out of range",
        )
        .await,
        "Invalid state - state must be between 0 and 3."
    );
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/friend?cursor=not-a-cursor"))
                .bearer_auth(&alice.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "friend list bad cursor",
        )
        .await,
        "Cursor is invalid."
    );

    assert_eq!(
        refusal(
            client
                .post(with_query(
                    &format!("{base}/v2/friend"),
                    &[("ids", alice.uid.as_str())],
                ))
                .bearer_auth(&alice.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "adding self as a friend",
        )
        .await,
        "Cannot add self as friend."
    );
    assert_eq!(
        refusal(
            client
                .post(with_query(
                    &format!("{base}/v2/friend"),
                    &[("usernames", "no-such-player-at-all")],
                ))
                .bearer_auth(&alice.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "adding an unresolvable username",
        )
        .await,
        "No valid ID or username was provided."
    );
    assert_eq!(
        refusal(
            client
                .post(with_query(
                    &format!("{base}/v2/friend"),
                    &[("ids", "not-a-uuid")],
                ))
                .bearer_auth(&alice.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "adding a malformed id",
        )
        .await,
        "Invalid user ID 'not-a-uuid'."
    );

    // Alice invites Bob, carrying edge metadata.
    assert_eq!(
        api_post_empty(
            client,
            with_query(
                &format!("{base}/v2/friend"),
                &[("ids", bob.uid.as_str()), ("metadata", r#"{"nick":"bob"}"#)],
            ),
            &alice.token,
            "alice invites bob",
        )
        .await,
        json!({})
    );
    assert_eq!(friend_edge_state(pool, a, b).await, Some(1));
    assert_eq!(friend_edge_state(pool, b, a).await, Some(2));
    assert_eq!(edge_count(pool, a).await, 1);
    assert_eq!(edge_count(pool, b).await, 1);

    // The invite is a notification to Bob.
    let inbox = api_get(
        client,
        format!(
            "{base}/v2/notification?limit=10&sender_id={}",
            alice.uid
        ),
        &bob.token,
        "bob's friend-request notification",
    )
    .await;
    let rows = inbox["notifications"]
        .as_array()
        .expect("bob has a notification");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["code"], -2);
    assert_eq!(rows[0]["sender_id"], alice.uid);
    assert_eq!(
        rows[0]["subject"],
        format!("{} wants to add you as a friend", alice.username)
    );
    assert_eq!(json_string(&rows[0]["content"])["username"], alice.username);
    assert_eq!(rows[0]["persistent"], true);

    let pending = api_get(
        client,
        format!("{base}/v2/friend?state=1"),
        &alice.token,
        "alice's sent invites",
    )
    .await;
    let rows = pending["friends"].as_array().expect("a sent invite");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["user"]["id"], bob.uid);
    assert_eq!(rows[0]["state"], 1);
    assert_eq!(json_string(&rows[0]["metadata"]), json!({ "nick": "bob" }));
    assert!(rows[0]["update_time"].as_str().is_some());

    // Bob adds Alice back: the pending pair is accepted rather than re-invited.
    assert_eq!(
        api_post_empty(
            client,
            with_query(
                &format!("{base}/v2/friend"),
                &[
                    ("ids", alice.uid.as_str()),
                    ("metadata", r#"{"since":"accepted"}"#),
                ],
            ),
            &bob.token,
            "bob accepts alice's invite",
        )
        .await,
        json!({})
    );
    assert_eq!(friend_edge_state(pool, a, b).await, Some(0));
    assert_eq!(friend_edge_state(pool, b, a).await, Some(0));
    assert_eq!(edge_count(pool, a).await, 1);
    assert_eq!(edge_count(pool, b).await, 1);
    let bob_meta: Value = sqlx::query_scalar(
        "SELECT metadata FROM user_edge WHERE source_id = $1 AND destination_id = $2",
    )
    .bind(b)
    .bind(a)
    .fetch_one(pool)
    .await
    .expect("bob's edge metadata");
    assert_eq!(bob_meta, json!({ "since": "accepted" }));

    let accepted = api_get(
        client,
        format!(
            "{base}/v2/notification?limit=10&sender_id={}",
            bob.uid
        ),
        &alice.token,
        "alice's acceptance notification",
    )
    .await;
    assert!(accepted["notifications"]
        .as_array()
        .expect("alice has notifications")
        .iter()
        .any(|n| n["code"] == -3
            && n["subject"] == format!("{} accepted your friend request", bob.username)));

    // Bob befriends Carol so Alice has a friend-of-a-friend.
    api_post_empty(
        client,
        with_query(
            &format!("{base}/v2/friend"),
            &[("ids", carol.uid.as_str())],
        ),
        &bob.token,
        "bob invites carol",
    )
    .await;
    api_post_empty(
        client,
        with_query(&format!("{base}/v2/friend"), &[("ids", bob.uid.as_str())]),
        &carol.token,
        "carol accepts bob's invite",
    )
    .await;
    assert_eq!(friend_edge_state(pool, b, c).await, Some(0));

    let fof = api_get(
        client,
        format!("{base}/v2/friend/friends?limit=10"),
        &alice.token,
        "alice's friends of friends",
    )
    .await;
    let entries = fof["friends_of_friends"]
        .as_array()
        .expect("a friends-of-friends page");
    let entry = entries
        .iter()
        .find(|entry| entry["user"]["id"] == carol.uid)
        .expect("carol is reachable through bob");
    assert_eq!(entry["referrer"], bob.uid);
    assert!(entries
        .iter()
        .all(|entry| entry["user"]["id"] != alice.uid && entry["user"]["id"] != bob.uid));
    assert_eq!(fof["cursor"], "");

    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/friend/friends?limit=0"))
                .bearer_auth(&alice.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "friends-of-friends limit below one",
        )
        .await,
        "Invalid limit - limit must be between 1 and 100."
    );

    // Blocking is one-directional and quietly discards a later invite.
    assert_eq!(
        refusal(
            client
                .post(with_query(
                    &format!("{base}/v2/friend/block"),
                    &[("ids", carol.uid.as_str())],
                ))
                .bearer_auth(&carol.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "blocking self",
        )
        .await,
        "Cannot block self."
    );
    assert_eq!(
        api_post_empty(
            client,
            with_query(
                &format!("{base}/v2/friend/block"),
                &[("ids", alice.uid.as_str())],
            ),
            &carol.token,
            "carol blocks alice",
        )
        .await,
        json!({})
    );
    assert_eq!(friend_edge_state(pool, c, a).await, Some(3));
    assert_eq!(edge_count(pool, c).await, 2);

    assert_eq!(
        api_post_empty(
            client,
            with_query(
                &format!("{base}/v2/friend"),
                &[("ids", carol.uid.as_str())],
            ),
            &alice.token,
            "alice invites a player who blocked her",
        )
        .await,
        json!({})
    );
    assert_eq!(
        friend_edge_state(pool, a, c).await,
        None,
        "a blocked invite writes nothing"
    );

    let blocked = api_get(
        client,
        format!("{base}/v2/friend?state=3"),
        &carol.token,
        "carol's blocked list",
    )
    .await;
    assert_eq!(blocked["friends"][0]["user"]["id"], alice.uid);
    assert_eq!(blocked["friends"][0]["state"], 3);

    // Deleting a friend removes both edges, both counts and notifies the other side.
    assert_eq!(
        api_delete(
            client,
            with_query(
                &format!("{base}/v2/friend"),
                &[("ids", carol.uid.as_str())],
            ),
            &bob.token,
            "bob deletes carol",
        )
        .await,
        json!({})
    );
    assert_eq!(friend_edge_state(pool, b, c).await, None);
    assert_eq!(friend_edge_state(pool, c, b).await, None);
    assert_eq!(edge_count(pool, b).await, 1);
    assert_eq!(edge_count(pool, c).await, 1);
    let removed = api_get(
        client,
        format!("{base}/v2/notification?limit=10&sender_id={}", bob.uid),
        &carol.token,
        "carol's removal notification",
    )
    .await;
    assert!(removed["notifications"]
        .as_array()
        .expect("carol has notifications")
        .iter()
        .any(|n| n["code"] == -9
            && n["subject"] == format!("{} removed you as a friend", bob.username)));

    assert_eq!(
        api_delete(
            client,
            with_query(
                &format!("{base}/v2/friend"),
                &[("ids", bob.uid.as_str())],
            ),
            &alice.token,
            "alice deletes bob",
        )
        .await,
        json!({})
    );
    assert_eq!(friend_edge_state(pool, a, b).await, None);
    assert_eq!(friend_edge_state(pool, b, a).await, None);
    assert_eq!(edge_count(pool, a).await, 0);
    assert_eq!(edge_count(pool, b).await, 0);

    assert_eq!(
        refusal(
            client
                .delete(with_query(
                    &format!("{base}/v2/friend"),
                    &[("ids", alice.uid.as_str())],
                ))
                .bearer_auth(&alice.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "deleting self as a friend",
        )
        .await,
        "Cannot delete self."
    );
    assert_eq!(
        api_delete(
            client,
            format!("{base}/v2/friend"),
            &alice.token,
            "deleting no friends",
        )
        .await,
        json!({})
    );

    // Imports are unconfigured in this deployment, and say so.
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/friend/facebook"))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "facebook import",
        )
        .await,
        "Facebook authentication is not configured."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/friend/steam"))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "steam import",
        )
        .await,
        "Steam authentication is not configured."
    );
}

// ---------------------------------------------------------------------------
// Groups
// ---------------------------------------------------------------------------

async fn group_surface(client: &Client, base: &str, pool: &PgPool) {
    eprintln!("e2e: groups");
    let owner = device_player(client, base, "gr-o").await;
    let member1 = device_player(client, base, "gr-m").await;
    let member2 = device_player(client, base, "gr-n").await;
    let outsider = device_player(client, base, "gr-x").await;

    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/group"))
                .bearer_auth(&owner.token)
                .json(&json!({ "name": "" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "creating a group without a name",
        )
        .await,
        "Group name must be set."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/group"))
                .bearer_auth(&owner.token)
                .json(&json!({ "name": format!("e2e-bad-{}", letters_suffix()), "max_count": -1 }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "creating a group with a negative size",
        )
        .await,
        "Group max count must be >= 1 when set."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/group"))
                .bearer_auth(&owner.token)
                .json(&json!({
                    "name": format!("e2e-bad-{}", letters_suffix()),
                    "creator_id": Uuid::new_v4().to_string(),
                }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "creating a group for another creator",
        )
        .await,
        "Group creator invalid."
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/group"))
                .bearer_auth(&owner.token)
                .json(&json!({ "name": format!("e2e-bad-{}", letters_suffix()), "metadata": "{not json" }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "creating a group with bad metadata",
        )
        .await,
        "Failed to unmarshal group metadata."
    );

    let open_name = format!("e2e-open-{}", letters_suffix());
    let created = api_post(
        client,
        format!("{base}/v2/group"),
        &owner.token,
        json!({
            "name": open_name,
            "open": true,
            "max_count": 2,
            "description": "the open group",
            "lang_tag": "en",
            "metadata": r#"{"tier":"gold"}"#,
        }),
        "create an open group",
    )
    .await;
    let open_id = created["group"]["id"].as_str().expect("group id").to_owned();
    let open = Uuid::parse_str(&open_id).expect("group uuid");
    assert_eq!(created["group"]["creator_id"], owner.uid);
    assert_eq!(created["group"]["name"], open_name);
    assert_eq!(created["group"]["open"], true);
    assert_eq!(created["group"]["edge_count"], 1);
    assert_eq!(created["group"]["max_count"], 2);
    assert_eq!(created["group"]["description"], "the open group");
    assert_eq!(created["group"]["lang_tag"], "en");
    assert_eq!(
        json_string(&created["group"]["metadata"]),
        json!({ "tier": "gold" })
    );

    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/group"))
                .bearer_auth(&owner.token)
                .json(&json!({ "name": open_name }))
                .send()
                .await
                .unwrap(),
            StatusCode::CONFLICT,
            "reusing a group name",
        )
        .await,
        "Group name is in use."
    );

    let fetched = api_get(
        client,
        format!("{base}/v2/group/{open_id}"),
        &owner.token,
        "get a group",
    )
    .await;
    assert_eq!(fetched["group"]["id"], open_id);
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/group/not-a-uuid"))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "getting a malformed group id",
        )
        .await,
        "Group ID must be a valid ID."
    );
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/group/{}", Uuid::new_v4()))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "getting an unknown group",
        )
        .await,
        "Group not found."
    );

    let by_name = api_get(
        client,
        with_query(&format!("{base}/v2/group"), &[("name", open_name.as_str())]),
        &owner.token,
        "searching groups by name",
    )
    .await;
    assert_eq!(by_name["groups"][0]["id"], open_id);
    assert_eq!(
        refusal(
            client
                .get(with_query(
                    &format!("{base}/v2/group"),
                    &[("name", open_name.as_str()), ("open", "true")],
                ))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "combining the name filter with another filter",
        )
        .await,
        "name filter cannot be combined with any other filter"
    );

    let discover = api_get(
        client,
        format!("{base}/v2/group?open=true&members=1"),
        &owner.token,
        "discovering open groups",
    )
    .await;
    assert!(discover["groups"]
        .as_array()
        .expect("group list")
        .iter()
        .any(|group| group["id"] == open_id));
    assert!(discover["groups"].as_array().unwrap().len() <= 1);

    let bad_bool = refusal(
        client
            .get(format!("{base}/v2/group?open=maybe"))
            .bearer_auth(&owner.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "an unparseable open filter",
    )
    .await;
    assert!(bad_bool.starts_with("invalid query parameter"), "{bad_bool}");
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/group?limit=0"))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "group list limit below one",
        )
        .await,
        "Invalid limit - limit must be between 1 and 100."
    );
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/group?cursor=not-a-cursor"))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "group list bad cursor",
        )
        .await,
        "Cursor is invalid."
    );

    // Capacity is claimed by the counter: the second member fills the group,
    // and the third is refused.
    assert_eq!(
        api_post_empty(
            client,
            format!("{base}/v2/group/{open_id}/join"),
            &member1.token,
            "member1 joins the open group",
        )
        .await,
        json!({})
    );
    assert_eq!(group_edge_state(pool, uuid_of(&member1), open).await, Some(2));
    assert_eq!(group_count(pool, open).await, 2);
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/group/{open_id}/join"))
                .bearer_auth(&member2.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "joining a full group",
        )
        .await,
        "Group is full."
    );

    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/group/{open_id}/leave"))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "the last superadmin leaving",
        )
        .await,
        "Cannot leave group when you are the last superadmin."
    );
    assert_eq!(
        api_post_empty(
            client,
            format!("{base}/v2/group/{open_id}/leave"),
            &member1.token,
            "member1 leaves the open group",
        )
        .await,
        json!({})
    );
    assert_eq!(group_count(pool, open).await, 1);
    assert_eq!(group_edge_state(pool, uuid_of(&member1), open).await, None);

    api_post_empty(
        client,
        format!("{base}/v2/group/{open_id}/join"),
        &member2.token,
        "member2 joins the freed slot",
    )
    .await;
    assert_eq!(group_edge_state(pool, uuid_of(&member2), open).await, Some(2));
    assert_eq!(group_count(pool, open).await, 2);

    // Banning a member frees the slot and locks the door.
    assert_eq!(
        api_post_empty(
            client,
            format!(
                "{base}/v2/group/{open_id}/ban?user_ids={}",
                member2.uid
            ),
            &owner.token,
            "owner bans member2",
        )
        .await,
        json!({})
    );
    assert_eq!(
        group_edge_state(pool, uuid_of(&member2), open).await,
        Some(4)
    );
    assert_eq!(group_count(pool, open).await, 1);
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/group/{open_id}/join"))
                .bearer_auth(&member2.token)
                .send()
                .await
                .unwrap(),
            StatusCode::FORBIDDEN,
            "a banned player rejoining",
        )
        .await,
        "Banned from the group."
    );

    // Adding a member directly, then promote/demote/kick around the role ladder.
    assert_eq!(
        api_post_empty(
            client,
            format!("{base}/v2/group/{open_id}/add"),
            &owner.token,
            "adding no users",
        )
        .await,
        json!({})
    );
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/group/{open_id}/add?user_ids=not-a-uuid"))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "adding a malformed user id",
        )
        .await,
        "User ID must be a valid ID."
    );
    assert_eq!(
        refusal(
            client
                .post(format!(
                    "{base}/v2/group/{open_id}/add?user_ids={}",
                    Uuid::new_v4()
                ))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "adding an unknown user",
        )
        .await,
        "One or more users not found."
    );
    assert_eq!(
        api_post_empty(
            client,
            format!("{base}/v2/group/{open_id}/add?user_ids={}", member1.uid),
            &owner.token,
            "owner adds member1",
        )
        .await,
        json!({})
    );
    assert_eq!(group_edge_state(pool, uuid_of(&member1), open).await, Some(2));
    assert_eq!(group_count(pool, open).await, 2);

    let roster = api_get(
        client,
        format!("{base}/v2/group/{open_id}/user?limit=100"),
        &owner.token,
        "the group roster",
    )
    .await;
    let roster = roster["group_users"].as_array().expect("a roster");
    assert_eq!(roster.len(), 2);
    assert_eq!(roster[0]["user"]["id"], owner.uid);
    assert_eq!(roster[0]["state"], 0);
    assert_eq!(roster[1]["user"]["id"], member1.uid);
    assert_eq!(roster[1]["state"], 2);

    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/group/{open_id}/user?state=5"))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "group roster state out of range",
        )
        .await,
        "Invalid state - state must be between 0 and 4."
    );
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/group/{open_id}/user?limit=0"))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "group roster limit below one",
        )
        .await,
        "Invalid limit - limit must be between 1 and 100."
    );
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/group/{open_id}/user?cursor=not-a-cursor"))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "group roster bad cursor",
        )
        .await,
        "Cursor is invalid."
    );

    api_post_empty(
        client,
        format!("{base}/v2/group/{open_id}/promote?user_ids={}", member1.uid),
        &owner.token,
        "owner promotes member1",
    )
    .await;
    assert_eq!(group_edge_state(pool, uuid_of(&member1), open).await, Some(1));
    let admins = api_get(
        client,
        format!("{base}/v2/group/{open_id}/user?state=1"),
        &owner.token,
        "the group's admins",
    )
    .await;
    let admins = admins["group_users"].as_array().expect("an admin page");
    assert_eq!(admins.len(), 1);
    assert_eq!(admins[0]["user"]["id"], member1.uid);

    assert_eq!(
        refusal(
            client
                .post(format!(
                    "{base}/v2/group/{open_id}/kick?user_ids={}",
                    owner.uid
                ))
                .bearer_auth(&member1.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an admin kicking the superadmin",
        )
        .await,
        "Cannot remove a user of equal or higher role."
    );

    api_post_empty(
        client,
        format!("{base}/v2/group/{open_id}/demote?user_ids={}", member1.uid),
        &owner.token,
        "owner demotes member1",
    )
    .await;
    assert_eq!(group_edge_state(pool, uuid_of(&member1), open).await, Some(2));

    assert_eq!(
        api_post_empty(
            client,
            format!("{base}/v2/group/{open_id}/kick?user_ids={}", member1.uid),
            &owner.token,
            "owner kicks member1",
        )
        .await,
        json!({})
    );
    assert_eq!(group_edge_state(pool, uuid_of(&member1), open).await, None);
    assert_eq!(group_count(pool, open).await, 1);
    let roster = api_get(
        client,
        format!("{base}/v2/group/{open_id}/user?limit=100"),
        &owner.token,
        "the roster after the kick",
    )
    .await;
    assert_eq!(roster["group_users"].as_array().unwrap().len(), 1);

    // A closed group records a join request; an admin's add accepts it.
    let closed_name = format!("e2e-closed-{}", letters_suffix());
    let closed = api_post(
        client,
        format!("{base}/v2/group"),
        &owner.token,
        json!({ "name": closed_name }),
        "create a closed group",
    )
    .await;
    let closed_id = closed["group"]["id"]
        .as_str()
        .expect("closed group id")
        .to_owned();
    let closed_uuid = Uuid::parse_str(&closed_id).expect("closed group uuid");
    assert_eq!(closed["group"]["open"], false);
    assert_eq!(closed["group"]["max_count"], 100);
    assert!(closed["group"].get("description").is_none());
    assert!(closed["group"].get("avatar_url").is_none());

    api_post_empty(
        client,
        format!("{base}/v2/group/{closed_id}/join"),
        &outsider.token,
        "outsider requests to join the closed group",
    )
    .await;
    assert_eq!(
        group_edge_state(pool, uuid_of(&outsider), closed_uuid).await,
        Some(3)
    );
    assert_eq!(group_count(pool, closed_uuid).await, 1);
    let request = api_get(
        client,
        format!(
            "{base}/v2/notification?limit=10&sender_id={}",
            outsider.uid
        ),
        &owner.token,
        "the owner's join-request notification",
    )
    .await;
    let request = request["notifications"]
        .as_array()
        .expect("the owner has notifications")
        .iter()
        .find(|n| n["code"] == -5)
        .expect("a join-request notification");
    let content = json_string(&request["content"]);
    assert_eq!(content["group_id"], closed_id);
    assert_eq!(content["username"], outsider.username);
    assert_eq!(
        request["subject"],
        format!("User {} wants to join your group", outsider.username)
    );

    api_post_empty(
        client,
        format!("{base}/v2/group/{closed_id}/join"),
        &outsider.token,
        "outsider requests again",
    )
    .await;
    assert_eq!(
        group_edge_state(pool, uuid_of(&outsider), closed_uuid).await,
        Some(3)
    );

    api_post_empty(
        client,
        format!("{base}/v2/group/{closed_id}/add?user_ids={}", outsider.uid),
        &owner.token,
        "owner accepts the join request",
    )
    .await;
    assert_eq!(
        group_edge_state(pool, uuid_of(&outsider), closed_uuid).await,
        Some(2)
    );
    assert_eq!(group_count(pool, closed_uuid).await, 2);
    let added = api_get(
        client,
        format!(
            "{base}/v2/notification?limit=10&sender_id={}",
            owner.uid
        ),
        &outsider.token,
        "the added player's notification",
    )
    .await;
    let added = added["notifications"]
        .as_array()
        .expect("the added player has notifications")
        .iter()
        .find(|n| n["code"] == -4)
        .expect("an added-to-group notification");
    assert_eq!(
        added["subject"],
        format!("You've been added to group {closed_name}")
    );
    assert_eq!(json_string(&added["content"])["group_id"], closed_id);

    // Updates: superadmin only, at least one field, name clashes are bad requests.
    assert_eq!(
        refusal(
            client
                .put(format!("{base}/v2/group/{closed_id}"))
                .bearer_auth(&owner.token)
                .json(&json!({}))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "an empty group update",
        )
        .await,
        "Specify at least one field to update."
    );
    assert_eq!(
        refusal(
            client
                .put(format!("{base}/v2/group/{closed_id}"))
                .bearer_auth(&owner.token)
                .json(&json!({ "name": open_name }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "renaming a group to a taken name",
        )
        .await,
        "Group name is in use."
    );
    assert_eq!(
        refusal(
            client
                .put(format!("{base}/v2/group/{closed_id}"))
                .bearer_auth(&owner.token)
                .json(&json!({ "max_count": 0 }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "shrinking a group to nothing",
        )
        .await,
        "Group max count must be >= 1 when set."
    );
    assert_eq!(
        refusal(
            client
                .put(format!("{base}/v2/group/{closed_id}"))
                .bearer_auth(&outsider.token)
                .json(&json!({ "description": "not mine" }))
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "a member updating a group",
        )
        .await,
        "Group not found or permission denied."
    );
    assert_eq!(
        api_put(
            client,
            format!("{base}/v2/group/{closed_id}"),
            &owner.token,
            json!({ "description": "updated", "open": true }),
            "owner updates the group",
        )
        .await,
        json!({})
    );
    let updated = api_get(
        client,
        format!("{base}/v2/group/{closed_id}"),
        &owner.token,
        "the updated group",
    )
    .await;
    assert_eq!(updated["group"]["description"], "updated");
    assert_eq!(updated["group"]["open"], true);

    // A player's own group list sees the edge state and skips soft-deleted rows.
    let groups = api_get(
        client,
        format!("{base}/v2/user/{}/group", outsider.uid),
        &owner.token,
        "the outsider's groups",
    )
    .await;
    assert!(groups["user_groups"]
        .as_array()
        .expect("a user-group page")
        .iter()
        .any(|entry| entry["group"]["id"] == closed_id && entry["state"] == 2));
    let only_member = api_get(
        client,
        format!("{base}/v2/user/{}/group?state=2", outsider.uid),
        &owner.token,
        "the outsider's memberships",
    )
    .await;
    assert!(only_member["user_groups"]
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| entry["state"] == 2));
    let none = api_get(
        client,
        format!("{base}/v2/user/{}/group?state=0", outsider.uid),
        &owner.token,
        "groups the outsider owns",
    )
    .await;
    assert_eq!(none["user_groups"], json!([]));
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/user/{}/group?state=5", outsider.uid))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "user-group state out of range",
        )
        .await,
        "Invalid state - state must be between 0 and 4."
    );
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/user/{}/group?limit=0", outsider.uid))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "user-group limit below one",
        )
        .await,
        "Invalid limit - limit must be between 1 and 100."
    );
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/user/not-a-uuid/group"))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "a malformed user id on the group alias",
        )
        .await,
        "Group ID must be a valid ID."
    );

    // Leaving removes the edge and one slot; leaving twice is not found.
    assert_eq!(
        api_post_empty(
            client,
            format!("{base}/v2/group/{closed_id}/leave"),
            &outsider.token,
            "outsider leaves the closed group",
        )
        .await,
        json!({})
    );
    assert_eq!(
        group_edge_state(pool, uuid_of(&outsider), closed_uuid).await,
        None
    );
    assert_eq!(group_count(pool, closed_uuid).await, 1);
    assert_eq!(
        refusal(
            client
                .post(format!("{base}/v2/group/{closed_id}/leave"))
                .bearer_auth(&outsider.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "leaving a group the caller is not in",
        )
        .await,
        "Group not found."
    );

    // Deletion is a superadmin action and is soft: reads stop finding the group.
    assert_eq!(
        refusal(
            client
                .delete(format!("{base}/v2/group/{closed_id}"))
                .bearer_auth(&member1.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "a non-member deleting the group",
        )
        .await,
        "Group not found or permission denied."
    );
    assert_eq!(
        api_delete(
            client,
            format!("{base}/v2/group/{closed_id}"),
            &owner.token,
            "owner deletes the closed group",
        )
        .await,
        json!({})
    );
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/group/{closed_id}"))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "getting a deleted group",
        )
        .await,
        "Group not found."
    );
    let owner_groups = api_get(
        client,
        format!("{base}/v2/user/{}/group?limit=100", owner.uid),
        &owner.token,
        "the owner's groups after a delete",
    )
    .await;
    assert!(owner_groups["user_groups"]
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| entry["group"]["id"] != closed_id));

    assert_eq!(
        api_delete(
            client,
            format!("{base}/v2/group/{open_id}"),
            &owner.token,
            "owner deletes the open group",
        )
        .await,
        json!({})
    );
    assert_eq!(
        refusal(
            client
                .get(format!("{base}/v2/group/{open_id}"))
                .bearer_auth(&owner.token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "getting the deleted open group",
        )
        .await,
        "Group not found."
    );
}

// ---------------------------------------------------------------------------
// Channels
// ---------------------------------------------------------------------------

async fn channel_surface(client: &Client, base: &str, pool: &PgPool) {
    eprintln!("e2e: channels");
    let alice = device_player(client, base, "ch-a").await;
    let bob = device_player(client, base, "ch-b").await;
    let charlie = device_player(client, base, "ch-c").await;

    let room_label = format!("e2e-room-{}", letters_suffix());
    let room = format!("2...{room_label}");

    // A malformed id never reaches a stream.
    for bad in [
        "not-a-channel".to_owned(),
        "1.aa.bb.cc".to_owned(),
        "3....".to_owned(),
        format!("2...{}", "r".repeat(65)),
    ] {
        refusal(
            client
                .get(format!("{base}/v2/channel/{bad}"))
                .bearer_auth(&alice.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "list a malformed channel",
        )
        .await;
    }

    let empty = api_get(
        client,
        with_query(&format!("{base}/v2/channel/{room}"), &[("limit", "10")]),
        &alice.token,
        "empty room listing",
    )
    .await;
    assert_eq!(empty["messages"], json!([]));
    assert!(empty.get("next_cursor").is_none());
    assert!(empty.get("cacheable_cursor").is_none());

    // Content must be an object or an array; everything else is refused.
    for bad in [json!({ "content": "plain" }), json!({ "content": 7 })] {
        refusal(
            client
                .post(format!("{base}/v2/channel/{room}/message"))
                .bearer_auth(&alice.token)
                .json(&bad)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "send a non-object message",
        )
        .await;
    }

    let first = api_post(
        client,
        format!("{base}/v2/channel/{room}/message"),
        &alice.token,
        json!({ "content": { "text": "hello" } }),
        "send the first room message",
    )
    .await;
    assert_eq!(first["channel_id"], room);
    assert_eq!(first["sender_id"], alice.uid);
    assert_eq!(first["username"], alice.username);
    assert_eq!(first["code"], 0);
    assert_eq!(first["persistent"], true);
    assert_eq!(first["room_name"], room_label);
    assert_eq!(json_string(&first["content"]), json!({ "text": "hello" }));
    let first_id = first["message_id"].as_str().expect("first message id").to_owned();

    let second = api_post(
        client,
        format!("{base}/v2/channel/{room}/message"),
        &bob.token,
        json!({ "content": [{ "text": "second" }] }),
        "send the second room message",
    )
    .await;
    let second_id = second["message_id"].as_str().expect("second message id").to_owned();
    assert_eq!(second["sender_id"], bob.uid);

    let third = api_post(
        client,
        format!("{base}/v2/channel/{room}/message"),
        &alice.token,
        json!({ "content": { "text": "third" } }),
        "send the third room message",
    )
    .await;
    let third_id = third["message_id"].as_str().expect("third message id").to_owned();

    // Forward pages walk oldest to newest and hand back the cursor that resumes.
    let page = api_get(
        client,
        with_query(&format!("{base}/v2/channel/{room}"), &[("limit", "2")]),
        &alice.token,
        "first forward page",
    )
    .await;
    let messages = page["messages"].as_array().expect("messages");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["message_id"], first_id);
    assert_eq!(messages[1]["message_id"], second_id);
    let next = page["next_cursor"]
        .as_str()
        .expect("forward page has a next cursor")
        .to_owned();
    assert!(page.get("prev_cursor").is_none());

    let page = api_get(
        client,
        with_query(
            &format!("{base}/v2/channel/{room}"),
            &[("limit", "2"), ("cursor", &next)],
        ),
        &alice.token,
        "second forward page",
    )
    .await;
    let messages = page["messages"].as_array().expect("messages");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["message_id"], third_id);
    assert!(page.get("next_cursor").is_none());
    assert!(page["prev_cursor"].is_string());

    // Backward pages walk newest to oldest.
    let page = api_get(
        client,
        with_query(
            &format!("{base}/v2/channel/{room}"),
            &[("limit", "2"), ("forward", "false")],
        ),
        &alice.token,
        "backward page",
    )
    .await;
    let messages = page["messages"].as_array().expect("messages");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["message_id"], third_id);
    assert_eq!(messages[1]["message_id"], second_id);

    // A cursor belongs to the stream and direction that issued it.
    refusal(
        client
            .get(with_query(
                &format!("{base}/v2/channel/{room}"),
                &[
                    ("limit", "2"),
                    ("cursor", &next),
                    ("forward", "false"),
                ],
            ))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "replay a forward cursor backward",
    )
    .await;
    refusal(
        client
            .get(with_query(
                &format!("{base}/v2/channel/{room}"),
                &[("cursor", "garbage")],
            ))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "list a channel with a malformed cursor",
    )
    .await;
    for limit in ["0", "101"] {
        refusal(
            client
                .get(with_query(
                    &format!("{base}/v2/channel/{room}"),
                    &[("limit", limit)],
                ))
                .bearer_auth(&alice.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "list a channel with a bad limit",
        )
        .await;
    }

    // Only the author may edit; removal is a tombstone, not a delete.
    refusal(
        client
            .put(format!("{base}/v2/channel/{room}/message/{first_id}"))
            .bearer_auth(&bob.token)
            .json(&json!({ "content": { "text": "hijack" } }))
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "edit another sender's message",
    )
    .await;
    let edited = api_put(
        client,
        format!("{base}/v2/channel/{room}/message/{first_id}"),
        &alice.token,
        json!({ "content": { "text": "edited" } }),
        "edit an own message",
    )
    .await;
    assert_eq!(json_string(&edited["content"]), json!({ "text": "edited" }));
    assert_eq!(edited["message_id"], first_id);

    refusal(
        client
            .put(format!("{base}/v2/channel/{room}/message/not-a-uuid"))
            .bearer_auth(&alice.token)
            .json(&json!({ "content": {} }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "edit a malformed message id",
    )
    .await;
    refusal(
        client
            .delete(format!("{base}/v2/channel/{room}/message/not-a-uuid"))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "delete a malformed message id",
    )
    .await;

    let tombstone = api_delete(
        client,
        format!("{base}/v2/channel/{room}/message/{first_id}"),
        &alice.token,
        "delete an own message",
    )
    .await;
    assert_eq!(tombstone["message_id"], first_id);
    assert_eq!(tombstone["code"], 2);

    // A DM is visible to exactly its two participants, in canonical id order.
    let (subject, descriptor) = if alice.uid < bob.uid {
        (alice.uid.clone(), bob.uid.clone())
    } else {
        (bob.uid.clone(), alice.uid.clone())
    };
    let dm = format!("4.{subject}.{descriptor}.");
    let dm_message = api_post(
        client,
        format!("{base}/v2/channel/{dm}/message"),
        &alice.token,
        json!({ "content": { "text": "psst" } }),
        "send a DM",
    )
    .await;
    assert_eq!(dm_message["user_id_one"], subject);
    assert_eq!(dm_message["user_id_two"], descriptor);
    assert!(dm_message.get("group_id").is_none());
    assert!(dm_message.get("room_name").is_none());

    let dm_page = api_get(
        client,
        format!("{base}/v2/channel/{dm}"),
        &bob.token,
        "read a DM as its other participant",
    )
    .await;
    assert_eq!(dm_page["messages"].as_array().expect("messages").len(), 1);
    refusal(
        client
            .get(format!("{base}/v2/channel/{dm}"))
            .bearer_auth(&charlie.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "read somebody else's DM",
    )
    .await;
    let reversed = format!("4.{descriptor}.{subject}.");
    refusal(
        client
            .get(format!("{base}/v2/channel/{reversed}"))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "use a non-canonical DM id",
    )
    .await;
    let self_dm = format!("4.{}.{}.", alice.uid, alice.uid);
    refusal(
        client
            .get(format!("{base}/v2/channel/{self_dm}"))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "address a DM to yourself",
    )
    .await;

    // A group channel is for its members only.
    let group_name = format!("e2e-channel-group-{}", letters_suffix());
    let group = api_post(
        client,
        format!("{base}/v2/group"),
        &alice.token,
        json!({ "name": group_name, "open": true }),
        "create a group for its channel",
    )
    .await;
    let gid = group["group"]["id"].as_str().expect("group id").to_owned();
    let gchannel = format!("3.{gid}..");
    api_post(
        client,
        format!("{base}/v2/channel/{gchannel}/message"),
        &alice.token,
        json!({ "content": { "text": "members only" } }),
        "send to a group channel",
    )
    .await;
    refusal(
        client
            .get(format!("{base}/v2/channel/{gchannel}"))
            .bearer_auth(&bob.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "read a group channel as a non-member",
    )
    .await;
    api_post_empty(
        client,
        format!("{base}/v2/group/{gid}/join"),
        &bob.token,
        "join the group to read its channel",
    )
    .await;
    let member_page = api_get(
        client,
        format!("{base}/v2/channel/{gchannel}"),
        &bob.token,
        "read a group channel as a member",
    )
    .await;
    assert_eq!(member_page["messages"].as_array().expect("messages").len(), 1);
    assert_eq!(member_page["messages"][0]["group_id"], gid);
    assert!(member_page["messages"][0].get("user_id_one").is_none());
    api_post_empty(
        client,
        format!("{base}/v2/group/{gid}/leave"),
        &bob.token,
        "leave the group channel",
    )
    .await;
    refusal(
        client
            .get(format!("{base}/v2/channel/{gchannel}"))
            .bearer_auth(&bob.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "read a group channel after leaving",
    )
    .await;

    let stored: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM message WHERE stream_label = $1 OR stream_subject = $2::uuid")
            .bind(&room_label)
            .bind(&gid)
            .fetch_one(pool)
            .await
            .expect("message rows");
    assert_eq!(stored, 4, "3 room rows plus 1 group row");
}

// ---------------------------------------------------------------------------
// Notifications
// ---------------------------------------------------------------------------

async fn notification_surface(client: &Client, base: &str, pool: &PgPool) {
    eprintln!("e2e: notifications");
    let alice = device_player(client, base, "no-a").await;
    let bob = device_player(client, base, "no-b").await;
    let charlie = device_player(client, base, "no-c").await;

    let add = |username: &str| {
        with_query(
            &format!("{base}/v2/friend"),
            &[("usernames", username)],
        )
    };

    // A friend invite lands on the recipient (code -2).
    api_post_empty(
        client,
        add(&bob.username),
        &alice.token,
        "alice invites bob",
    )
    .await;
    // Bob accepting lands on alice (code -3).
    api_post_empty(
        client,
        add(&alice.username),
        &bob.token,
        "bob accepts alice",
    )
    .await;

    let group_name = format!("e2e-notify-group-{}", letters_suffix());
    let group = api_post(
        client,
        format!("{base}/v2/group"),
        &alice.token,
        json!({ "name": group_name, "open": false }),
        "create a closed group for notifications",
    )
    .await;
    let gid = group["group"]["id"].as_str().expect("group id").to_owned();
    // A join request lands on the group's admins (code -5).
    api_post_empty(
        client,
        format!("{base}/v2/group/{gid}/join"),
        &bob.token,
        "bob requests to join",
    )
    .await;
    // Being added lands on the added player (code -4).
    api_post_empty(
        client,
        with_query(
            &format!("{base}/v2/group/{gid}/add"),
            &[("user_ids", &bob.uid)],
        ),
        &alice.token,
        "alice adds bob",
    )
    .await;
    // Removing a friend lands on the removed player (code -9).
    api_delete(
        client,
        add(&bob.username),
        &alice.token,
        "alice removes bob",
    )
    .await;

    let bob_page = api_get(
        client,
        with_query(&format!("{base}/v2/notification"), &[("limit", "100")]),
        &bob.token,
        "bob's notifications",
    )
    .await;
    let rows = bob_page["notifications"]
        .as_array()
        .expect("bob has notifications")
        .clone();
    assert_eq!(rows.len(), 3, "{bob_page}");
    let mut codes: Vec<i64> = rows
        .iter()
        .map(|row| row["code"].as_i64().expect("a code"))
        .collect();
    codes.sort_unstable();
    assert_eq!(codes, vec![-9, -4, -2]);
    for row in &rows {
        assert_eq!(row["sender_id"], alice.uid);
        assert_eq!(row["persistent"], true);
        assert!(row["create_time"].as_str().is_some_and(|t| t.ends_with('Z')));
    }
    let by_code = |page: &Value, code: i16| -> Value {
        page["notifications"]
            .as_array()
            .expect("notifications")
            .iter()
            .find(|row| row["code"] == code)
            .unwrap_or_else(|| panic!("no notification with code {code}: {page}"))
            .clone()
    };
    let invite = by_code(&bob_page, -2);
    assert_eq!(json_string(&invite["content"]), json!({ "username": alice.username }));
    assert_eq!(
        invite["subject"],
        format!("{} wants to add you as a friend", alice.username)
    );
    let added = by_code(&bob_page, -4);
    assert_eq!(
        json_string(&added["content"]),
        json!({ "group_id": gid, "name": group_name })
    );
    assert_eq!(added["subject"], format!("You've been added to group {group_name}"));
    let removed = by_code(&bob_page, -9);
    assert_eq!(json_string(&removed["content"]), json!({ "username": alice.username }));
    assert_eq!(
        removed["subject"],
        format!("{} removed you as a friend", alice.username)
    );

    // The other side of the same events.
    let alice_page = api_get(
        client,
        with_query(&format!("{base}/v2/notification"), &[("limit", "100")]),
        &alice.token,
        "alice's notifications",
    )
    .await;
    let alice_rows = alice_page["notifications"]
        .as_array()
        .expect("alice has notifications");
    assert_eq!(alice_rows.len(), 2);
    let mut alice_codes: Vec<i64> = alice_rows
        .iter()
        .map(|row| row["code"].as_i64().expect("a code"))
        .collect();
    alice_codes.sort_unstable();
    assert_eq!(alice_codes, vec![-5, -3]);
    let requested = by_code(&alice_page, -5);
    assert_eq!(json_string(&requested["content"])["group_id"], gid);
    assert_eq!(json_string(&requested["content"])["username"], bob.username);
    assert_eq!(
        requested["subject"],
        format!("User {} wants to join your group", bob.username)
    );
    let accepted = by_code(&alice_page, -3);
    assert_eq!(json_string(&accepted["content"]), json!({ "username": bob.username }));

    // Filters and paging.
    let filtered = api_get(
        client,
        with_query(
            &format!("{base}/v2/notification"),
            &[("limit", "100"), ("code", "-2")],
        ),
        &bob.token,
        "filter notifications by code",
    )
    .await;
    assert_eq!(filtered["notifications"].as_array().expect("notifications").len(), 1);
    assert_eq!(filtered["notifications"][0]["code"], -2);
    let by_sender = api_get(
        client,
        with_query(
            &format!("{base}/v2/notification"),
            &[("limit", "100"), ("sender_id", &alice.uid)],
        ),
        &bob.token,
        "filter notifications by sender",
    )
    .await;
    assert_eq!(by_sender["notifications"].as_array().expect("notifications").len(), 3);
    refusal(
        client
            .get(with_query(
                &format!("{base}/v2/notification"),
                &[("sender_id", "not-a-uuid")],
            ))
            .bearer_auth(&bob.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "filter notifications by a malformed sender",
    )
    .await;
    for limit in ["0", "101"] {
        refusal(
            client
                .get(with_query(
                    &format!("{base}/v2/notification"),
                    &[("limit", limit)],
                ))
                .bearer_auth(&bob.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "list notifications with a bad limit",
        )
        .await;
    }
    // The gateway hides every list-query failure behind one internal error.
    refusal(
        client
            .get(with_query(
                &format!("{base}/v2/notification"),
                &[("cacheableCursor", "garbage")],
            ))
            .bearer_auth(&bob.token)
            .send()
            .await
            .unwrap(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "list notifications with a malformed cursor",
    )
    .await;

    // The default page is one notification.
    let default_page = api_get(
        client,
        format!("{base}/v2/notification"),
        &alice.token,
        "default notification page",
    )
    .await;
    assert_eq!(default_page["notifications"].as_array().expect("notifications").len(), 1);

    // Walk all three bob notifications one at a time.
    let mut seen: Vec<String> = Vec::new();
    let mut cursor = String::new();
    for _ in 0..3 {
        let mut pairs: Vec<(&str, &str)> = vec![("limit", "1")];
        if !cursor.is_empty() {
            pairs.push(("cacheableCursor", &cursor));
        }
        let page = api_get(
            client,
            with_query(&format!("{base}/v2/notification"), &pairs),
            &bob.token,
            "page through notifications",
        )
        .await;
        let rows = page["notifications"].as_array().expect("notifications");
        assert_eq!(rows.len(), 1);
        seen.push(rows[0]["id"].as_str().expect("notification id").to_owned());
        cursor = page["cacheable_cursor"]
            .as_str()
            .expect("a cacheable cursor")
            .to_owned();
    }
    assert_eq!(seen.len(), 3);
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 3);

    let charlie_page = api_get(
        client,
        with_query(&format!("{base}/v2/notification"), &[("limit", "100")]),
        &charlie.token,
        "a player with no notifications",
    )
    .await;
    assert!(charlie_page.get("notifications").is_none());
    assert!(!charlie_page["cacheable_cursor"].as_str().unwrap_or("").is_empty());

    // Deleting by id, then by every accepted list spelling.
    let single = removed["id"].as_str().expect("notification id").to_owned();
    api_delete(
        client,
        format!("{base}/v2/notification/{single}"),
        &bob.token,
        "delete one notification",
    )
    .await;
    refusal(
        client
            .delete(format!("{base}/v2/notification/not-a-uuid"))
            .bearer_auth(&bob.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "delete a malformed notification id",
    )
    .await;
    refusal(
        client
            .delete(format!("{base}/v2/notification/{}", Uuid::new_v4()))
            .bearer_auth(&bob.token)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "delete an unknown notification",
    )
    .await;

    let remaining: Vec<String> = bob_page["notifications"]
        .as_array()
        .expect("notifications")
        .iter()
        .filter_map(|row| row["id"].as_str().map(str::to_owned))
        .filter(|id| id != &single)
        .collect();
    assert_eq!(remaining.len(), 2);
    // Repeated query keys cannot be captured by a struct field; the route merges them.
    api_delete(
        client,
        with_query(
            &format!("{base}/v2/notification"),
            &[("ids", &remaining[0]), ("ids", &remaining[1])],
        ),
        &bob.token,
        "delete notifications by repeated ids",
    )
    .await;
    let cleared = api_get(
        client,
        with_query(&format!("{base}/v2/notification"), &[("limit", "100")]),
        &bob.token,
        "bob has no notifications left",
    )
    .await;
    assert!(cleared.get("notifications").is_none());
    assert!(!cleared["cacheable_cursor"].as_str().unwrap_or("").is_empty());

    // A JSON body (the SDK's shape) deletes too — alice invites bob again.
    api_post_empty(client, add(&bob.username), &alice.token, "invite bob again").await;
    let invite = api_get(
        client,
        with_query(&format!("{base}/v2/notification"), &[("limit", "100")]),
        &bob.token,
        "the second invite",
    )
    .await;
    let invite_id = invite["notifications"][0]["id"]
        .as_str()
        .expect("a notification id")
        .to_owned();
    api_delete_body(
        client,
        format!("{base}/v2/notification"),
        &bob.token,
        json!({ "ids": [invite_id] }),
        "delete notifications by JSON body",
    )
    .await;

    // A JSON array spelled into one query value deletes too.
    api_delete(client, add(&bob.username), &alice.token, "remove bob again").await;
    let removed = api_get(
        client,
        with_query(&format!("{base}/v2/notification"), &[("limit", "100")]),
        &bob.token,
        "the second removal",
    )
    .await;
    let removed_id = removed["notifications"][0]["id"]
        .as_str()
        .expect("a notification id")
        .to_owned();
    let array_value = json!([removed_id]).to_string();
    api_delete(
        client,
        with_query(&format!("{base}/v2/notification"), &[("ids", &array_value)]),
        &bob.token,
        "delete notifications by a JSON-array query value",
    )
    .await;

    // A comma-joined list deletes both rows in one request.
    api_post_empty(client, add(&bob.username), &alice.token, "invite bob once more").await;
    api_delete(client, add(&bob.username), &alice.token, "remove bob once more").await;
    let pairs = api_get(
        client,
        with_query(&format!("{base}/v2/notification"), &[("limit", "100")]),
        &bob.token,
        "the final pair of notifications",
    )
    .await;
    let ids: Vec<String> = pairs["notifications"]
        .as_array()
        .expect("notifications")
        .iter()
        .map(|row| row["id"].as_str().expect("id").to_owned())
        .collect();
    assert_eq!(ids.len(), 2);
    api_delete(
        client,
        with_query(
            &format!("{base}/v2/notification"),
            &[("ids", &ids.join(","))],
        ),
        &bob.token,
        "delete notifications by a comma-joined list",
    )
    .await;

    // Nothing named deletes nothing, successfully.
    api_delete_body(
        client,
        format!("{base}/v2/notification"),
        &bob.token,
        json!({ "ids": [] }),
        "delete an empty notification list",
    )
    .await;

    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notification WHERE user_id = $1")
        .bind(uuid_of(&bob))
        .fetch_one(pool)
        .await
        .expect("notification rows");
    assert_eq!(left, 0);
}

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

fn md5_hex(raw: &str) -> String {
    use md5::{Digest, Md5};
    let mut hasher = Md5::new();
    hasher.update(raw.as_bytes());
    format!("{:x}", hasher.finalize())
}

async fn storage_surface(client: &Client, base: &str) {
    eprintln!("e2e: storage");
    let alice = device_player(client, base, "st-a").await;
    let bob = device_player(client, base, "st-b").await;
    let collection = format!("e2e-storage-{}", letters_suffix());
    let storage = format!("{base}/v2/storage");

    let value1 = json!({ "n": 1 }).to_string();
    let value2 = json!({ "n": 2 }).to_string();
    let value3 = json!({ "n": 3 }).to_string();
    let version1 = md5_hex(&value1);
    let version2 = md5_hex(&value2);
    let version3 = md5_hex(&value3);

    // An empty batch is a successful no-op.
    let empty = api_put(
        client,
        storage.clone(),
        &alice.token,
        json!({ "objects": [] }),
        "write an empty batch",
    )
    .await;
    assert_eq!(empty, json!({}));

    // The checks that run before any statement.
    for (body, what) in [
        (
            json!({ "objects": [{ "collection": "", "key": "a", "value": value1 }] }),
            "a write with no collection",
        ),
        (
            json!({ "objects": [{ "collection": collection, "key": "", "value": value1 }] }),
            "a write with no key",
        ),
        (
            json!({ "objects": [{ "collection": collection, "key": "a", "value": "" }] }),
            "a write with no value",
        ),
        (
            json!({ "objects": [{ "collection": collection, "key": "a", "value": "[]" }] }),
            "a write with a non-object value",
        ),
        (
            json!({ "objects": [{ "collection": collection, "key": "a", "value": value1, "permission_read": 3 }] }),
            "a write with an out-of-range read permission",
        ),
        (
            json!({ "objects": [{ "collection": collection, "key": "a", "value": value1, "permission_write": 2 }] }),
            "a write with an out-of-range write permission",
        ),
    ] {
        refusal(
            client
                .put(storage.clone())
                .bearer_auth(&alice.token)
                .json(&body)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            what,
        )
        .await;
    }

    let ack = api_put(
        client,
        storage.clone(),
        &alice.token,
        json!({ "objects": [{ "collection": collection, "key": "a", "value": value1 }] }),
        "write a private object",
    )
    .await;
    let object = &ack["acks"][0];
    assert_eq!(object["collection"], collection);
    assert_eq!(object["key"], "a");
    assert_eq!(object["user_id"], alice.uid);
    assert_eq!(object["version"], version1);

    // An upsert with a public read permission replaces the value.
    api_put(
        client,
        storage.clone(),
        &alice.token,
        json!({ "objects": [{ "collection": collection, "key": "a", "value": value2, "permission_read": 2 }] }),
        "upsert a public object",
    )
    .await;

    // Compare-and-set: a stale token is refused, the current one applies.
    refusal(
        client
            .put(storage.clone())
            .bearer_auth(&alice.token)
            .json(&json!({ "objects": [{ "collection": collection, "key": "a", "value": value3, "version": "deadbeef" }] }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "write with a stale version",
    )
    .await;
    let cas = api_put(
        client,
        storage.clone(),
        &alice.token,
        json!({ "objects": [{ "collection": collection, "key": "a", "value": value3, "version": version2, "permission_read": 2 }] }),
        "write with the current version",
    )
    .await;
    assert_eq!(cas["acks"][0]["version"], version3);

    // Writes are applied in collection/key order but acknowledged in request
    // order, so a client can zip acks back onto its own object array.
    let batch = api_put(
        client,
        storage.clone(),
        &alice.token,
        json!({ "objects": [
            { "collection": collection, "key": "sort-z", "value": json!({ "z": 1 }).to_string() },
            { "collection": collection, "key": "sort-a", "value": json!({ "a": 1 }).to_string() },
        ] }),
        "write an out-of-order batch",
    )
    .await;
    let keys: Vec<String> = batch["acks"]
        .as_array()
        .expect("acks")
        .iter()
        .map(|ack| ack["key"].as_str().expect("key").to_owned())
        .collect();
    assert_eq!(keys, vec!["sort-z", "sort-a"]);

    // Insert-only, private and read-only objects.
    let b_value = json!({ "b": 1 }).to_string();
    let c_value = json!({ "c": 1 }).to_string();
    api_put(
        client,
        storage.clone(),
        &alice.token,
        json!({ "objects": [
            { "collection": collection, "key": "b", "value": b_value },
            { "collection": collection, "key": "c", "value": c_value, "permission_read": 2 },
            { "collection": collection, "key": "ro", "value": json!({ "ro": 1 }).to_string(), "permission_read": 2, "permission_write": 0 },
            { "collection": collection, "key": "star", "value": json!({ "star": 1 }).to_string(), "version": "*" },
        ] }),
        "write the remaining objects",
    )
    .await;
    refusal(
        client
            .put(storage.clone())
            .bearer_auth(&alice.token)
            .json(&json!({ "objects": [{ "collection": collection, "key": "star", "value": json!({ "other": 1 }).to_string(), "version": "*" }] }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "insert-only write over an existing key",
    )
    .await;
    refusal(
        client
            .put(storage.clone())
            .bearer_auth(&alice.token)
            .json(&json!({ "objects": [{ "collection": collection, "key": "ro", "value": json!({ "ro": 2 }).to_string() }] }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "overwrite a read-only object",
    )
    .await;

    // Another account may keep its own object under the same key, without
    // touching the owner's.
    api_put(
        client,
        storage.clone(),
        &bob.token,
        json!({ "objects": [{ "collection": collection, "key": "a", "value": json!({ "bob": 1 }).to_string(), "permission_read": 2 }] }),
        "bob writes his own object under a shared key",
    )
    .await;
    let unchanged = api_get(
        client,
        format!("{base}/v2/storage/{collection}/a/{}", alice.uid),
        &alice.token,
        "alice's object survived bob's write",
    )
    .await;
    // jsonb re-serializes on the way out, so compare structurally.
    assert_eq!(json_string(&unchanged["value"]), json!({ "n": 3 }));
    assert_eq!(unchanged["user_id"], alice.uid);
    assert_eq!(unchanged["version"], version3);
    assert_eq!(unchanged["permission_read"], 2);
    assert_eq!(unchanged["permission_write"], 1);

    // Batch reads hide what the caller may not read.
    let read = api_post(
        client,
        storage.clone(),
        &alice.token,
        json!({ "objectIds": [
            { "collection": collection, "key": "a" },
            { "collection": collection, "key": "b" },
        ] }),
        "read two own objects",
    )
    .await;
    let keys: Vec<String> = read["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|object| object["key"].as_str().expect("key").to_owned())
        .collect();
    assert_eq!(keys, vec!["a", "b"]);
    assert_eq!(json_string(&read["objects"][0]["value"]), json!({ "n": 3 }));

    let bob_read = api_post(
        client,
        storage.clone(),
        &bob.token,
        json!({ "object_ids": [
            { "collection": collection, "key": "a" },
            { "collection": collection, "key": "b" },
        ] }),
        "read another account's objects",
    )
    .await;
    let keys: Vec<String> = bob_read["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|object| object["key"].as_str().expect("key").to_owned())
        .collect();
    assert_eq!(
        keys,
        vec!["a"],
        "an omitted owner means the caller; alice's private b stays hidden"
    );
    assert_eq!(
        json_string(&bob_read["objects"][0]["value"]),
        json!({ "bob": 1 }),
        "bob's own object under the shared key"
    );
    assert_eq!(bob_read["objects"][0]["user_id"], bob.uid);

    let empty_read = api_post(
        client,
        storage.clone(),
        &alice.token,
        json!({ "object_ids": [] }),
        "read an empty batch",
    )
    .await;
    assert_eq!(empty_read, json!({}));
    for (body, what) in [
        (
            json!({ "object_ids": [{ "collection": "", "key": "a" }] }),
            "read with no collection",
        ),
        (
            json!({ "object_ids": [{ "collection": collection, "key": "a", "user_id": "00000000-0000-0000-0000-000000000000" }] }),
            "read under the nil owner",
        ),
        (
            json!({ "object_ids": [{ "collection": collection, "key": "a", "user_id": "not-a-uuid" }] }),
            "read under a malformed owner",
        ),
    ] {
        refusal(
            client
                .post(storage.clone())
                .bearer_auth(&alice.token)
                .json(&body)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            what,
        )
        .await;
    }

    // One object by its full key: the nil owner names the caller.
    let one = api_get(
        client,
        format!(
            "{base}/v2/storage/{collection}/a/00000000-0000-0000-0000-000000000000"
        ),
        &alice.token,
        "read an own object by its full key",
    )
    .await;
    assert_eq!(json_string(&one["value"]), json!({ "n": 3 }));
    let versioned = api_get(
        client,
        with_query(
            &format!("{base}/v2/storage/{collection}/a/{}", alice.uid),
            &[("version", &version3)],
        ),
        &alice.token,
        "read an object at its current version",
    )
    .await;
    assert_eq!(versioned["version"], version3);
    refusal(
        client
            .get(with_query(
                &format!("{base}/v2/storage/{collection}/a/{}", alice.uid),
                &[("version", "deadbeef")],
            ))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "read an object at a superseded version",
    )
    .await;
    refusal(
        client
            .get(format!("{base}/v2/storage/{collection}/b/{}", alice.uid))
            .bearer_auth(&bob.token)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "read another account's private object",
    )
    .await;
    let public = api_get(
        client,
        format!("{base}/v2/storage/{collection}/a/{}", alice.uid),
        &bob.token,
        "read another account's public object",
    )
    .await;
    assert_eq!(json_string(&public["value"]), json!({ "n": 3 }));
    refusal(
        client
            .get(format!("{base}/v2/storage/{collection}/a/not-a-uuid"))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "read an object under a malformed owner",
    )
    .await;

    // Public listings page by key.
    let page = api_get(
        client,
        with_query(
            &format!("{base}/v2/storage/{collection}"),
            &[("limit", "2")],
        ),
        &bob.token,
        "first public listing page",
    )
    .await;
    let keys: Vec<String> = page["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|object| object["key"].as_str().expect("key").to_owned())
        .collect();
    // Both accounts' public `a` rows are visible, ordered by key then owner.
    assert_eq!(keys, vec!["a", "a"]);
    let cursor = page["cursor"].as_str().expect("a listing cursor").to_owned();
    let page = api_get(
        client,
        with_query(
            &format!("{base}/v2/storage/{collection}"),
            &[("limit", "2"), ("cursor", &cursor)],
        ),
        &bob.token,
        "second public listing page",
    )
    .await;
    let keys: Vec<String> = page["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|object| object["key"].as_str().expect("key").to_owned())
        .collect();
    assert_eq!(keys, vec!["c", "ro"]);
    assert!(page.get("cursor").is_none());
    refusal(
        client
            .get(with_query(
                &format!("{base}/v2/storage/{collection}"),
                &[("cursor", "garbage")],
            ))
            .bearer_auth(&bob.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "list storage with a malformed cursor",
    )
    .await;
    for limit in ["0", "101"] {
        refusal(
            client
                .get(with_query(
                    &format!("{base}/v2/storage/{collection}"),
                    &[("limit", limit)],
                ))
                .bearer_auth(&bob.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "list storage with a bad limit",
        )
        .await;
    }

    // An own listing includes private objects; another account's does not.
    let own = api_get(
        client,
        with_query(
            &format!("{base}/v2/storage/{collection}/{}", alice.uid),
            &[("limit", "100")],
        ),
        &alice.token,
        "alice's own listing",
    )
    .await;
    let mut keys: Vec<String> = own["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|object| object["key"].as_str().expect("key").to_owned())
        .collect();
    keys.sort();
    assert_eq!(keys, vec!["a", "b", "c", "ro", "sort-a", "sort-z", "star"]);
    let other = api_get(
        client,
        with_query(
            &format!("{base}/v2/storage/{collection}/{}", alice.uid),
            &[("limit", "100")],
        ),
        &bob.token,
        "another account's listing",
    )
    .await;
    let mut keys: Vec<String> = other["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|object| object["key"].as_str().expect("key").to_owned())
        .collect();
    keys.sort();
    assert_eq!(keys, vec!["a", "c", "ro"]);
    refusal(
        client
            .get(format!(
                "{base}/v2/storage/{collection}/not-a-uuid"
            ))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "list under a malformed owner",
    )
    .await;

    // Deletes are versioned and owner-scoped.
    refusal(
        client
            .put(format!("{base}/v2/storage/delete"))
            .bearer_auth(&alice.token)
            .json(&json!({ "object_ids": [{ "collection": collection, "key": "c", "version": "deadbeef" }] }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "delete with a stale version",
    )
    .await;
    refusal(
        client
            .put(format!("{base}/v2/storage/delete"))
            .bearer_auth(&bob.token)
            .json(&json!({ "object_ids": [{ "collection": collection, "key": "b" }] }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "delete another account's object",
    )
    .await;
    refusal(
        client
            .put(format!("{base}/v2/storage/delete"))
            .bearer_auth(&alice.token)
            .json(&json!({ "object_ids": [{ "collection": collection, "key": "missing" }] }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "delete a missing object",
    )
    .await;
    refusal(
        client
            .put(format!("{base}/v2/storage/delete"))
            .bearer_auth(&alice.token)
            .json(&json!({ "object_ids": [{ "collection": collection, "key": "ro" }] }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "delete a read-only object",
    )
    .await;
    let deleted = api_put(
        client,
        format!("{base}/v2/storage/delete"),
        &alice.token,
        json!({ "object_ids": [{ "collection": collection, "key": "c", "version": md5_hex(&c_value) }] }),
        "delete with the current version",
    )
    .await;
    assert_eq!(deleted, json!({}));
    refusal(
        client
            .get(format!("{base}/v2/storage/{collection}/c/{}", alice.uid))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "read a deleted object",
    )
    .await;
    let deleted = api_put(
        client,
        format!("{base}/v2/storage/delete"),
        &alice.token,
        json!({ "object_ids": [{ "collection": collection, "key": "a" }] }),
        "delete an object by key alone",
    )
    .await;
    assert_eq!(deleted, json!({}));
    let empty_delete = api_put(
        client,
        format!("{base}/v2/storage/delete"),
        &alice.token,
        json!({ "object_ids": [] }),
        "delete an empty batch",
    )
    .await;
    assert_eq!(empty_delete, json!({}));
}

// ---------------------------------------------------------------------------
// Storage indexes
// ---------------------------------------------------------------------------

async fn storage_index_surface(client: &Client, base: &str) {
    eprintln!("e2e: storage indexes");
    let alice = device_player(client, base, "si-a").await;
    let bob = device_player(client, base, "si-b").await;
    let suffix = letters_suffix();
    let collection = format!("e2e-index-{suffix}");
    let index_name = format!("e2e-idx-{suffix}");
    let index_only_name = format!("e2e-idx-only-{suffix}");
    let window_name = format!("e2e-idx-window-{suffix}");
    let index_url = format!("{base}/v2/storage/index");

    let objects = [
        ("k1", json!({ "level": 12, "name": "sword", "score": 30 })),
        ("k2", json!({ "level": 18, "name": "shield", "score": 10 })),
        ("k3", json!({ "level": 25, "name": "bow", "score": 20 })),
    ];
    let writes: Vec<Value> = objects
        .iter()
        .map(|(key, value)| {
            json!({
                "collection": collection,
                "key": key,
                "value": value.to_string(),
            })
        })
        .collect();
    api_put(
        client,
        format!("{base}/v2/storage"),
        &alice.token,
        json!({ "objects": writes }),
        "seed indexed objects",
    )
    .await;

    // Every definition rule runs before the insert.
    for (body, what) in [
        (
            json!({ "collection": collection, "fields": ["level"] }),
            "an index with no name",
        ),
        (
            json!({ "name": index_name, "fields": ["level"] }),
            "an index with no collection",
        ),
        (
            json!({ "name": index_name, "collection": collection, "fields": [] }),
            "an index with no fields",
        ),
        (
            json!({
                "name": index_name,
                "collection": collection,
                "fields": ["a", "b", "c", "d", "e"],
            }),
            "an index with too many fields",
        ),
        (
            json!({
                "name": index_name,
                "collection": collection,
                "fields": ["level"],
                "max_entries": 0,
            }),
            "an index with a non-positive window",
        ),
    ] {
        refusal(
            client
                .post(index_url.clone())
                .bearer_auth(&alice.token)
                .json(&body)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            what,
        )
        .await;
    }

    let created = api_post(
        client,
        index_url.clone(),
        &alice.token,
        json!({
            "name": index_name,
            "collection": collection,
            "fields": ["level", "name", "score"],
            "sortable_fields": ["score"],
        }),
        "create an index",
    )
    .await;
    assert_eq!(created["index"], index_name);
    assert_eq!(created["collection"], collection);
    assert_eq!(created["max_entries"], 10000);
    assert_eq!(created["fields"], json!(["level", "name", "score"]));
    assert_eq!(created["sortable_fields"], json!(["score"]));
    assert!(created.get("index_only").is_none());

    refusal(
        client
            .post(index_url.clone())
            .bearer_auth(&alice.token)
            .json(&json!({
                "name": index_name,
                "collection": collection,
                "fields": ["level"],
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::CONFLICT,
        "create a clashing index name",
    )
    .await;

    let listed = api_get(client, index_url.clone(), &alice.token, "list indexes").await;
    let names: Vec<String> = listed["indexes"]
        .as_array()
        .expect("indexes")
        .iter()
        .map(|index| index["index"].as_str().expect("index name").to_owned())
        .collect();
    assert!(names.contains(&index_name));

    // The query language and its refusals.
    let query_url = format!("{base}/v2/storage/index/{index_name}/query");
    for (body, status, what) in [
        (
            json!({ "query": "badterm" }),
            StatusCode::BAD_REQUEST,
            "parse a malformed index query",
        ),
        (
            json!({ "query": "missing:1" }),
            StatusCode::BAD_REQUEST,
            "query a field the index does not cover",
        ),
        (
            json!({ "query": "", "limit": 0 }),
            StatusCode::BAD_REQUEST,
            "query an index with a bad limit",
        ),
        (
            json!({ "query": "", "order": ["level"] }),
            StatusCode::BAD_REQUEST,
            "order by a field the index cannot sort",
        ),
    ] {
        refusal(
            client
                .post(query_url.clone())
                .bearer_auth(&alice.token)
                .json(&body)
                .send()
                .await
                .unwrap(),
            status,
            what,
        )
        .await;
    }

    let matched = api_post(
        client,
        query_url.clone(),
        &alice.token,
        json!({ "query": "level:12" }),
        "query an equality term",
    )
    .await;
    let keys: Vec<String> = matched["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|object| object["key"].as_str().expect("key").to_owned())
        .collect();
    assert_eq!(keys, vec!["k1"]);
    assert_eq!(json_string(&matched["objects"][0]["value"])["name"], "sword");

    let ranged = api_post(
        client,
        query_url.clone(),
        &alice.token,
        json!({ "query": "score:[15 TO 30]" }),
        "query a numeric range",
    )
    .await;
    let mut keys: Vec<String> = ranged["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|object| object["key"].as_str().expect("key").to_owned())
        .collect();
    keys.sort();
    assert_eq!(keys, vec!["k1", "k3"]);

    let sorted = api_post(
        client,
        query_url.clone(),
        &alice.token,
        json!({ "query": "", "order": ["-score"] }),
        "query ordered by a descending field",
    )
    .await;
    let keys: Vec<String> = sorted["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|object| object["key"].as_str().expect("key").to_owned())
        .collect();
    assert_eq!(keys, vec!["k1", "k3", "k2"]);
    let prefixed = api_post(
        client,
        query_url.clone(),
        &alice.token,
        json!({ "query": "", "order": ["-value.score"] }),
        "query ordered through the value prefix",
    )
    .await;
    assert_eq!(prefixed["objects"], sorted["objects"]);

    // The cursor replays query, limit and order; any drift is refused.
    let first_page = api_post(
        client,
        query_url.clone(),
        &alice.token,
        json!({ "query": "", "limit": 2 }),
        "first indexed page",
    )
    .await;
    assert_eq!(first_page["objects"].as_array().expect("objects").len(), 2);
    let cursor = first_page["next_cursor"]
        .as_str()
        .expect("an index cursor")
        .to_owned();
    let second_page = api_post(
        client,
        query_url.clone(),
        &alice.token,
        json!({ "query": "", "limit": 2, "cursor": cursor }),
        "second indexed page",
    )
    .await;
    let keys: Vec<String> = second_page["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|object| object["key"].as_str().expect("key").to_owned())
        .collect();
    assert_eq!(keys, vec!["k3"]);
    assert!(second_page.get("next_cursor").is_none());
    refusal(
        client
            .post(query_url.clone())
            .bearer_auth(&alice.token)
            .json(&json!({ "query": "level:12", "limit": 2, "cursor": cursor }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "replay an index cursor against another query",
    )
    .await;

    // Private objects stay invisible to another account.
    let hidden = api_post(
        client,
        query_url.clone(),
        &bob.token,
        json!({ "query": "" }),
        "query an index as another account",
    )
    .await;
    assert_eq!(hidden, json!({}));

    // An index-only read projects just the declared keys.
    api_post(
        client,
        index_url.clone(),
        &alice.token,
        json!({
            "name": index_only_name,
            "collection": collection,
            "fields": ["level"],
            "index_only": true,
        }),
        "create an index-only definition",
    )
    .await;
    let projected = api_post(
        client,
        format!("{base}/v2/storage/index/{index_only_name}/query"),
        &alice.token,
        json!({ "query": "" }),
        "read an index-only projection",
    )
    .await;
    for object in projected["objects"].as_array().expect("objects") {
        let value = json_string(&object["value"]);
        assert_eq!(value.as_object().expect("object").len(), 1);
        assert!(value.get("level").is_some());
    }

    // A small window caps the whole result set, cursor included.
    api_post(
        client,
        index_url.clone(),
        &alice.token,
        json!({
            "name": window_name,
            "collection": collection,
            "fields": ["level"],
            "max_entries": 2,
        }),
        "create a two-entry index",
    )
    .await;
    let capped = api_post(
        client,
        format!("{base}/v2/storage/index/{window_name}/query"),
        &alice.token,
        json!({ "query": "" }),
        "read a capped index",
    )
    .await;
    assert_eq!(capped["objects"].as_array().expect("objects").len(), 2);
    let cursor = capped["next_cursor"]
        .as_str()
        .expect("a capped index cursor")
        .to_owned();
    let exhausted = api_post(
        client,
        format!("{base}/v2/storage/index/{window_name}/query"),
        &alice.token,
        json!({ "query": "", "cursor": cursor }),
        "read past the capped window",
    )
    .await;
    assert_eq!(exhausted, json!({}));

    // Only the creator drops a definition; a missing one is not-found.
    refusal(
        client
            .delete(format!("{base}/v2/storage/index/{index_name}"))
            .bearer_auth(&bob.token)
            .send()
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "delete another account's index",
    )
    .await;
    refusal(
        client
            .delete(format!("{base}/v2/storage/index/absent-{suffix}"))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "delete an unknown index",
    )
    .await;
    for name in [&index_name, &index_only_name, &window_name] {
        api_delete(
            client,
            format!("{base}/v2/storage/index/{name}"),
            &alice.token,
            "delete an index",
        )
        .await;
    }
    refusal(
        client
            .post(query_url)
            .bearer_auth(&alice.token)
            .json(&json!({ "query": "" }))
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "query a deleted index",
    )
    .await;
    let listed = api_get(client, index_url, &alice.token, "indexes after deletion").await;
    let names: Vec<String> = listed["indexes"]
        .as_array()
        .map(|indexes| {
            indexes
                .iter()
                .map(|index| index["index"].as_str().expect("index name").to_owned())
                .collect()
        })
        .unwrap_or_default();
    assert!(!names.contains(&index_name));
}

// ---------------------------------------------------------------------------
// Inventory
// ---------------------------------------------------------------------------

async fn inventory_surface(client: &Client, base: &str) {
    eprintln!("e2e: inventory");
    let alice = device_player(client, base, "inv-a").await;
    let bob = device_player(client, base, "inv-b").await;
    let item_id = format!("e2e-item-{}", letters_suffix());
    let inventory = format!("{base}/v2/inventory");

    let empty = api_get(client, inventory.clone(), &alice.token, "empty inventory").await;
    assert_eq!(empty["items"], json!([]));

    let defined = api_post(
        client,
        format!("{inventory}/define"),
        &alice.token,
        json!({
            "item_id": item_id,
            "name": "E2E Sword",
            "category": "weapon",
            "max_stack": 5,
            "rarity": "rare",
            "properties": { "damage": 12 },
        }),
        "define an item",
    )
    .await;
    assert_eq!(defined["success"], true);
    assert_eq!(
        defined["message"],
        format!("item definition '{item_id}' registered")
    );
    assert!(defined["item"].is_null());

    refusal(
        client
            .post(format!("{inventory}/grant"))
            .bearer_auth(&alice.token)
            .json(&json!({ "item_id": format!("{item_id}-missing") }))
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "grant an undefined item",
    )
    .await;

    let granted = api_post(
        client,
        format!("{inventory}/grant"),
        &alice.token,
        json!({ "item_id": item_id, "quantity": 99, "metadata": { "bound": true } }),
        "grant an item beyond its stack",
    )
    .await;
    assert_eq!(granted["message"], "granted 5x E2E Sword");
    assert_eq!(granted["item"]["quantity"], 5);
    assert_eq!(granted["item"]["slot_index"], -1);
    assert_eq!(granted["item"]["user_id"], alice.uid);
    assert_eq!(granted["item"]["metadata"], json!({ "bound": true }));
    let first_instance = granted["item"]["instance_id"]
        .as_str()
        .expect("instance id")
        .to_owned();

    let zero = api_post(
        client,
        format!("{inventory}/grant"),
        &alice.token,
        json!({ "item_id": item_id, "quantity": 0 }),
        "grant zero of an item",
    )
    .await;
    assert_eq!(zero["item"]["quantity"], 1);
    let second_instance = zero["item"]["instance_id"]
        .as_str()
        .expect("instance id")
        .to_owned();

    let bob_item = api_post(
        client,
        format!("{inventory}/grant"),
        &alice.token,
        json!({ "user_id": bob.uid, "item_id": item_id, "quantity": 2 }),
        "grant an item to another account",
    )
    .await;
    assert_eq!(bob_item["item"]["user_id"], bob.uid);
    assert_eq!(bob_item["item"]["quantity"], 2);
    let bob_instance = bob_item["item"]["instance_id"]
        .as_str()
        .expect("instance id")
        .to_owned();

    refusal(
        client
            .post(format!("{inventory}/equip"))
            .bearer_auth(&alice.token)
            .json(&json!({ "instance_id": Uuid::new_v4(), "slot_index": 0 }))
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "equip an unknown item",
    )
    .await;
    refusal(
        client
            .post(format!("{inventory}/equip"))
            .bearer_auth(&alice.token)
            .json(&json!({ "instance_id": bob_instance, "slot_index": 0 }))
            .send()
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "equip another account's item",
    )
    .await;
    let equipped = api_post(
        client,
        format!("{inventory}/equip"),
        &alice.token,
        json!({ "instance_id": first_instance, "slot_index": 3 }),
        "equip an own item",
    )
    .await;
    assert_eq!(equipped["success"], true);
    assert_eq!(equipped["message"], "item equipped to slot 3");
    assert_eq!(equipped["item"]["slot_index"], 3);

    refusal(
        client
            .post(format!("{inventory}/consume"))
            .bearer_auth(&alice.token)
            .json(&json!({ "instance_id": first_instance, "quantity": 0 }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "consume nothing",
    )
    .await;
    refusal(
        client
            .post(format!("{inventory}/consume"))
            .bearer_auth(&alice.token)
            .json(&json!({ "instance_id": Uuid::new_v4(), "quantity": 1 }))
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "consume an unknown item",
    )
    .await;
    refusal(
        client
            .post(format!("{inventory}/consume"))
            .bearer_auth(&alice.token)
            .json(&json!({ "instance_id": bob_instance, "quantity": 1 }))
            .send()
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "consume another account's item",
    )
    .await;
    refusal(
        client
            .post(format!("{inventory}/consume"))
            .bearer_auth(&alice.token)
            .json(&json!({ "instance_id": first_instance, "quantity": 6 }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "consume more than the stack holds",
    )
    .await;
    let consumed = api_post(
        client,
        format!("{inventory}/consume"),
        &alice.token,
        json!({ "instance_id": first_instance, "quantity": 2 }),
        "consume part of a stack",
    )
    .await;
    assert_eq!(consumed["message"], "consumed 2 items");
    assert_eq!(consumed["item"]["quantity"], 3);
    let consumed = api_post(
        client,
        format!("{inventory}/consume"),
        &alice.token,
        json!({ "instance_id": first_instance, "quantity": 3 }),
        "consume the rest of a stack",
    )
    .await;
    assert_eq!(consumed["message"], "consumed 3 items");
    assert!(consumed["item"].is_null(), "an emptied stack is removed");

    let listed = api_get(client, inventory.clone(), &alice.token, "alice's inventory").await;
    let items = listed["items"].as_array().expect("items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["instance_id"], second_instance);
    assert_eq!(items[0]["quantity"], 1);

    let bob_listing = api_get(client, inventory, &bob.token, "bob's inventory").await;
    let items = bob_listing["items"].as_array().expect("items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["instance_id"], bob_instance);
}

// ---------------------------------------------------------------------------
// Leaderboards
// ---------------------------------------------------------------------------

async fn leaderboard_surface(client: &Client, base: &str, pool: &PgPool) {
    eprintln!("e2e: leaderboards");
    let alice = device_player(client, base, "lb-a").await;
    let bob = device_player(client, base, "lb-b").await;
    let charlie = device_player(client, base, "lb-c").await;
    let suffix = letters_suffix();
    let desc_id = format!("e2e-lb-desc-{suffix}");
    let ops_id = format!("e2e-lb-ops-{suffix}");
    let asc_id = format!("e2e-lb-asc-{suffix}");
    let auth_id = format!("e2e-lb-auth-{suffix}");
    let unranked_id = format!("e2e-lb-unranked-{suffix}");
    let tournament_id = format!("e2e-lb-round-{suffix}");

    for (id, sort_order, authoritative, enable_ranks) in [
        (&desc_id, 1_i16, false, true),
        (&ops_id, 1, false, true),
        (&asc_id, 0, false, true),
        (&auth_id, 1, true, true),
        (&unranked_id, 1, false, false),
    ] {
        sqlx::query(
            "INSERT INTO leaderboard (id, title, category, duration, sort_order, authoritative, enable_ranks) \
             VALUES ($1, $2, 0, 0, $3, $4, $5)",
        )
        .bind(id)
        .bind(id)
        .bind(sort_order)
        .bind(authoritative)
        .bind(enable_ranks)
        .execute(pool)
        .await
        .expect("seed a leaderboard");
    }
    sqlx::query(
        "INSERT INTO leaderboard (id, title, category, duration, start_time, end_time) \
         VALUES ($1, $2, 1, 60, now(), now() + interval '1 hour')",
    )
    .bind(&tournament_id)
    .bind(&tournament_id)
    .execute(pool)
    .await
    .expect("seed a tournament board");

    let url = |id: &str| format!("{base}/v2/leaderboard/{id}");

    // An absent body is not a record.
    refusal(
        client
            .post(url(&desc_id))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "write a record with no body",
    )
    .await;
    let first = api_post(
        client,
        url(&desc_id),
        &alice.token,
        json!({ "score": 100, "metadata": "{\"tag\":\"first\"}" }),
        "write the top record",
    )
    .await;
    assert_eq!(first["leaderboard_id"], desc_id);
    assert_eq!(first["owner_id"], alice.uid);
    assert_eq!(first["username"], alice.username);
    assert_eq!(first["score"], "100");
    assert_eq!(first["num_score"], 1);
    assert_eq!(first["rank"], "1");
    assert_eq!(json_string(&first["metadata"]), json!({ "tag": "first" }));
    assert!(first.get("expiry_time").is_none());

    // A best write only climbs.
    let lower = api_post(
        client,
        url(&desc_id),
        &alice.token,
        json!({ "score": 50 }),
        "write a lower best score",
    )
    .await;
    assert_eq!(lower["score"], "100");
    assert_eq!(lower["num_score"], 1);

    api_post(
        client,
        url(&desc_id),
        &bob.token,
        json!({ "score": 50 }),
        "write bob's record",
    )
    .await;
    api_post(
        client,
        url(&desc_id),
        &charlie.token,
        json!({ "score": 75 }),
        "write charlie's record",
    )
    .await;

    let page = api_get(
        client,
        with_query(&url(&desc_id), &[("limit", "2")]),
        &alice.token,
        "first leaderboard page",
    )
    .await;
    let scores: Vec<&str> = page["records"]
        .as_array()
        .expect("records")
        .iter()
        .map(|record| record["score"].as_str().expect("score"))
        .collect();
    assert_eq!(scores, vec!["100", "75"]);
    assert_eq!(page["records"][0]["rank"], "1");
    assert_eq!(page["records"][1]["rank"], "2");
    assert_eq!(page["rank_count"], "3");
    let cursor = page["next_cursor"]
        .as_str()
        .expect("a leaderboard cursor")
        .to_owned();
    let page = api_get(
        client,
        with_query(&url(&desc_id), &[("limit", "2"), ("cursor", &cursor)]),
        &alice.token,
        "second leaderboard page",
    )
    .await;
    let records = page["records"].as_array().expect("records");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["owner_id"], bob.uid);
    assert_eq!(records[0]["rank"], "3");
    assert!(page["prev_cursor"].is_string());
    assert!(page.get("next_cursor").is_none());

    let default_page = api_get(client, url(&desc_id), &alice.token, "default leaderboard page").await;
    assert_eq!(default_page["records"].as_array().expect("records").len(), 1);

    // Naming owners without paging answers only those records, in board order.
    let owners = format!("{},{}", alice.uid, bob.uid);
    let owned = api_get(
        client,
        with_query(&url(&desc_id), &[("owner_ids", &owners)]),
        &alice.token,
        "owner records batch",
    )
    .await;
    assert!(owned.get("records").is_none());
    let owner_scores: Vec<&str> = owned["owner_records"]
        .as_array()
        .expect("owner records")
        .iter()
        .map(|record| record["score"].as_str().expect("score"))
        .collect();
    assert_eq!(owner_scores, vec!["100", "50"]);

    for (pairs, what) in [
        (vec![("owner_ids", "not-a-uuid")], "list with a malformed owner"),
        (vec![("limit", "0")], "list with a zero limit"),
        (vec![("limit", "1001")], "list above the page ceiling"),
        (vec![("limit", "abc")], "list with a non-numeric limit"),
        (vec![("cursor", "garbage")], "list with a malformed cursor"),
        (vec![("expiry", "abc")], "list with a non-numeric expiry"),
    ] {
        refusal(
            client
                .get(with_query(&url(&desc_id), &pairs))
                .bearer_auth(&alice.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            what,
        )
        .await;
    }
    let expired = api_get(
        client,
        with_query(&url(&desc_id), &[("expiry", "1")]),
        &alice.token,
        "list a closed window",
    )
    .await;
    assert_eq!(expired, json!({}));

    // A haystack anchors on the owner and trims to the limit.
    let haystack = api_get(
        client,
        with_query(&url(&desc_id), &[("limit", "3")]),
        &alice.token,
        "haystack without an owner",
    )
    .await;
    // Owner-less haystack is a plain listing; the owner route is below.
    assert_eq!(haystack["records"].as_array().expect("records").len(), 3);
    let around = api_get(
        client,
        with_query(
            &format!("{base}/v2/leaderboard/{desc_id}/owner/{}", alice.uid),
            &[("limit", "3")],
        ),
        &alice.token,
        "alice's haystack",
    )
    .await;
    let scores: Vec<&str> = around["records"]
        .as_array()
        .expect("records")
        .iter()
        .map(|record| record["score"].as_str().expect("score"))
        .collect();
    assert_eq!(scores, vec!["100", "75", "50"]);
    assert_eq!(around["rank_count"], "3");
    let trimmed = api_get(
        client,
        with_query(
            &format!("{base}/v2/leaderboard/{desc_id}/owner/{}", bob.uid),
            &[("limit", "1")],
        ),
        &bob.token,
        "bob's trimmed haystack",
    )
    .await;
    let records = trimmed["records"].as_array().expect("records");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["owner_id"], bob.uid);
    assert_eq!(records[0]["rank"], "3");
    assert!(trimmed["prev_cursor"].is_string());
    let cursor_page = api_get(
        client,
        with_query(
            &format!("{base}/v2/leaderboard/{desc_id}/owner/{}", alice.uid),
            &[("limit", "2"), ("cursor", &cursor)],
        ),
        &alice.token,
        "a haystack handed a cursor",
    )
    .await;
    assert!(cursor_page.get("owner_records").is_none());
    assert_eq!(cursor_page["records"].as_array().expect("records").len(), 1);
    let no_record = api_get(
        client,
        format!(
            "{base}/v2/leaderboard/{desc_id}/owner/{}",
            Uuid::new_v4()
        ),
        &alice.token,
        "a haystack for a player with no record",
    )
    .await;
    assert_eq!(no_record, json!({}));
    for (pairs, what) in [
        (vec![("limit", "0")], "haystack with a zero limit"),
        (vec![("limit", "101")], "haystack above its ceiling"),
        (vec![("expiry", "abc")], "haystack with a non-numeric expiry"),
    ] {
        refusal(
            client
                .get(with_query(
                    &format!("{base}/v2/leaderboard/{desc_id}/owner/{}", alice.uid),
                    &pairs,
                ))
                .bearer_auth(&alice.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            what,
        )
        .await;
    }
    refusal(
        client
            .get(format!(
                "{base}/v2/leaderboard/{desc_id}/owner/not-a-uuid"
            ))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "haystack for a malformed owner",
    )
    .await;

    // Operator overrides on a board of their own.
    let ops = |score: i64, operator: i64| {
        json!({ "score": score, "operator": operator })
    };
    let written = api_post(client, url(&ops_id), &alice.token, ops(10, 0), "best 10").await;
    assert_eq!(written["score"], "10");
    api_post(client, url(&ops_id), &alice.token, ops(30, 0), "best 30").await;
    let unchanged = api_post(client, url(&ops_id), &alice.token, ops(5, 0), "best 5").await;
    assert_eq!(unchanged["score"], "30");
    assert_eq!(unchanged["num_score"], 2);
    let set = api_post(client, url(&ops_id), &alice.token, ops(40, 2), "set 40").await;
    assert_eq!(set["score"], "40");
    assert_eq!(set["num_score"], 3);
    let increment = api_post(client, url(&ops_id), &alice.token, ops(45, 3), "increment 45").await;
    assert_eq!(increment["score"], "85");
    let decrement = api_post(client, url(&ops_id), &alice.token, ops(100, 4), "decrement 100").await;
    assert!(decrement.get("score").is_none(), "a decrement floors at zero");
    refusal(
        client
            .post(url(&ops_id))
            .bearer_auth(&alice.token)
            .json(&ops(1, 9))
            .send()
            .await
            .unwrap(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "write with an unknown operator",
    )
    .await;
    refusal(
        client
            .post(url(&ops_id))
            .bearer_auth(&alice.token)
            .json(&json!({ "score": 1, "metadata": "[1]" }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "write a non-object metadata value",
    )
    .await;
    let empty_record = api_post(client, url(&ops_id), &alice.token, json!({}), "empty record").await;
    assert_eq!(empty_record["owner_id"], alice.uid);
    let removed = api_delete(
        client,
        url(&ops_id),
        &alice.token,
        "delete an own record",
    )
    .await;
    assert_eq!(removed, json!({}));
    let after_delete = api_get(
        client,
        with_query(&url(&ops_id), &[("owner_ids", &alice.uid)]),
        &alice.token,
        "list after deleting the only record",
    )
    .await;
    assert_eq!(after_delete, json!({}));

    // An ascending board sorts the other way.
    for (player, score) in [(&alice, 10), (&bob, 5), (&charlie, 7)] {
        api_post(
            client,
            url(&asc_id),
            &player.token,
            json!({ "score": score }),
            "write an ascending record",
        )
        .await;
    }
    let ascending = api_get(
        client,
        with_query(&url(&asc_id), &[("limit", "10")]),
        &alice.token,
        "ascending listing",
    )
    .await;
    let scores: Vec<&str> = ascending["records"]
        .as_array()
        .expect("records")
        .iter()
        .map(|record| record["score"].as_str().expect("score"))
        .collect();
    assert_eq!(scores, vec!["5", "7", "10"]);

    // An authoritative board belongs to the operator.
    refusal(
        client
            .post(url(&auth_id))
            .bearer_auth(&alice.token)
            .json(&json!({ "score": 1 }))
            .send()
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "write to an authoritative leaderboard",
    )
    .await;
    refusal(
        client
            .delete(url(&auth_id))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "delete from an authoritative leaderboard",
    )
    .await;

    // A board that keeps no ranks reports none.
    let unranked = api_post(
        client,
        url(&unranked_id),
        &alice.token,
        json!({ "score": 12 }),
        "write to an unranked board",
    )
    .await;
    assert!(unranked.get("rank").is_none());
    let unranked_list = api_get(
        client,
        with_query(&url(&unranked_id), &[("limit", "10")]),
        &alice.token,
        "list an unranked board",
    )
    .await;
    assert!(unranked_list.get("rank_count").is_none());
    assert!(unranked_list["records"][0].get("rank").is_none());

    // A round board is not a leaderboard for the delete route.
    refusal(
        client
            .delete(url(&tournament_id))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "delete a tournament record through the leaderboard route",
    )
    .await;

    // Unknown boards are not-found on every verb.
    for response in [
        client
            .post(url("e2e-lb-absent"))
            .bearer_auth(&alice.token)
            .json(&json!({ "score": 1 }))
            .send()
            .await
            .unwrap(),
        client
            .get(url("e2e-lb-absent"))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        client
            .delete(url("e2e-lb-absent"))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
    ] {
        refusal(response, StatusCode::NOT_FOUND, "address an unknown leaderboard").await;
    }

    // The round board is a tournament, so it would appear in the tournament
    // listings that run later against the same database.
    sqlx::query("DELETE FROM leaderboard WHERE id = $1")
        .bind(&tournament_id)
        .execute(pool)
        .await
        .expect("remove the seeded round board");
}

// ---------------------------------------------------------------------------
// IAP
// ---------------------------------------------------------------------------

async fn iap_surface(client: &Client, base: &str, pool: &PgPool) {
    eprintln!("e2e: iap");
    let alice = device_player(client, base, "iap-a").await;
    let suffix = letters_suffix();
    let iap = format!("{base}/v2/iap");

    refusal(
        client
            .post(iap.clone())
            .bearer_auth(&alice.token)
            .json(&json!({ "receipt": "" }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "validate an empty Apple receipt",
    )
    .await;

    let receipt = format!("e2e-receipt-{suffix}");
    let purchase = api_post(
        client,
        iap.clone(),
        &alice.token,
        json!({ "receipt": receipt, "product_id": "coin", "store": 0 }),
        "record a legacy Apple purchase",
    )
    .await;
    assert_eq!(purchase["product_id"], "coin");
    assert_eq!(purchase["transaction_id"], md5_hex(&receipt));
    assert_eq!(purchase["environment"], "UNKNOWN");
    assert!(purchase["purchase_time"].as_i64().expect("purchase time") > 0);
    assert!(purchase["update_time"].as_i64().expect("update time") > 0);
    assert!(purchase.get("refund_time").is_none());
    assert!(purchase.get("provider_notification").is_none());

    // A transaction id is bound to its first product for good.
    let replay = api_post(
        client,
        iap.clone(),
        &alice.token,
        json!({ "receipt": receipt, "product_id": "gem" }),
        "replay an Apple receipt",
    )
    .await;
    assert_eq!(replay["product_id"], "coin");
    assert_eq!(replay["transaction_id"], md5_hex(&receipt));

    let explicit = api_post(
        client,
        format!("{iap}/purchase/apple"),
        &alice.token,
        json!({ "receipt": format!("e2e-apple-{suffix}"), "product_id": "sword", "transaction_id": "e2e-tx-explicit" }),
        "record an Apple purchase with an explicit transaction id",
    )
    .await;
    assert_eq!(explicit["transaction_id"], "e2e-tx-explicit");
    assert_eq!(explicit["product_id"], "sword");

    refusal(
        client
            .post(format!("{iap}/purchase/google"))
            .bearer_auth(&alice.token)
            .json(&json!({ "purchase_token": "" }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "record a Google purchase with no token",
    )
    .await;
    let google = api_post(
        client,
        format!("{iap}/purchase/google"),
        &alice.token,
        json!({ "purchase_token": format!("e2e-google-{suffix}"), "product_id": "gem" }),
        "record a Google purchase",
    )
    .await;
    assert_eq!(google["transaction_id"], md5_hex(&format!("e2e-google-{suffix}")));
    assert_eq!(google["product_id"], "gem");

    let huawei = api_post(
        client,
        format!("{iap}/purchase/huawei"),
        &alice.token,
        json!({ "purchase_token": format!("e2e-huawei-{suffix}"), "product_id": "gem" }),
        "record a Huawei purchase",
    )
    .await;
    assert_eq!(huawei["transaction_id"], md5_hex(&format!("e2e-huawei-{suffix}")));

    let facebook = api_post(
        client,
        format!("{iap}/purchase/facebookinstant"),
        &alice.token,
        json!({ "signed_request": format!("e2e-facebook-{suffix}"), "product_id": "gem" }),
        "record a Facebook Instant purchase through its alias",
    )
    .await;
    assert_eq!(
        facebook["transaction_id"],
        md5_hex(&format!("e2e-facebook-{suffix}"))
    );

    let samsung = api_post(
        client,
        format!("{iap}/purchase/samsung"),
        &alice.token,
        json!({ "purchase_id": format!("e2e-samsung-{suffix}"), "product_id": "gem" }),
        "record a Samsung purchase through its alias",
    )
    .await;
    assert_eq!(
        samsung["transaction_id"],
        md5_hex(&format!("e2e-samsung-{suffix}"))
    );

    // Purchase paging: six rows, three per page.
    let first = api_get(
        client,
        with_query(&format!("{iap}/purchase"), &[("limit", "3")]),
        &alice.token,
        "first purchase page",
    )
    .await;
    let mut seen: Vec<String> = first["purchases"]
        .as_array()
        .expect("purchases")
        .iter()
        .map(|row| row["transaction_id"].as_str().expect("transaction").to_owned())
        .collect();
    assert_eq!(seen.len(), 3);
    let cursor = first["next_cursor"]
        .as_str()
        .expect("a purchase cursor")
        .to_owned();
    assert_eq!(cursor, base64::engine::general_purpose::STANDARD.encode("3"));
    let second = api_get(
        client,
        with_query(
            &format!("{iap}/purchase"),
            &[("limit", "3"), ("cursor", &cursor)],
        ),
        &alice.token,
        "second purchase page",
    )
    .await;
    seen.extend(
        second["purchases"]
            .as_array()
            .expect("purchases")
            .iter()
            .map(|row| row["transaction_id"].as_str().expect("transaction").to_owned()),
    );
    assert_eq!(seen.len(), 6);
    let cursor = second["next_cursor"]
        .as_str()
        .expect("the page was full")
        .to_owned();
    let third = api_get(
        client,
        with_query(
            &format!("{iap}/purchase"),
            &[("limit", "3"), ("cursor", &cursor)],
        ),
        &alice.token,
        "third purchase page",
    )
    .await;
    assert!(third["purchases"].as_array().expect("purchases").is_empty());
    assert_eq!(third["next_cursor"], "");

    // A malformed cursor decodes to the first page rather than failing.
    let recovered = api_get(
        client,
        with_query(&format!("{iap}/purchase"), &[("limit", "3"), ("cursor", "garbage")]),
        &alice.token,
        "purchase page with a malformed cursor",
    )
    .await;
    assert_eq!(recovered["purchases"].as_array().expect("purchases").len(), 3);

    // A refunded purchase leaves the listing.
    sqlx::query("UPDATE purchase SET refund_time = now() WHERE transaction_id = $1")
        .bind(&explicit["transaction_id"].as_str().expect("transaction"))
        .execute(pool)
        .await
        .expect("refund a purchase");
    let listed = api_get(
        client,
        with_query(&format!("{iap}/purchase"), &[("limit", "100")]),
        &alice.token,
        "purchases after a refund",
    )
    .await;
    assert_eq!(listed["purchases"].as_array().expect("purchases").len(), 5);

    // Subscriptions.
    refusal(
        client
            .post(format!("{iap}/subscription/apple"))
            .bearer_auth(&alice.token)
            .json(&json!({ "receipt": "" }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "record a subscription with no credential",
    )
    .await;

    let apple_token = format!("e2e-sub-apple-{suffix}");
    let subscription = api_post(
        client,
        format!("{iap}/subscription/apple"),
        &alice.token,
        json!({
            "receipt": apple_token,
            "product_id": "sub-a",
            "expire_time": "2030-01-01T00:00:00Z",
        }),
        "record an Apple subscription",
    )
    .await;
    assert_eq!(subscription["product_id"], "sub-a");
    assert_eq!(subscription["transaction_id"], md5_hex(&apple_token));
    assert_eq!(subscription["original_transaction_id"], md5_hex(&apple_token));
    assert_eq!(subscription["expiration_time"], 1_893_456_000_i64);
    assert_eq!(subscription["state"], 1);
    assert_eq!(subscription["environment"], "UNKNOWN");
    assert!(subscription.get("refund_time").is_none());

    let google_token = format!("e2e-sub-google-{suffix}");
    let google_sub = api_post(
        client,
        format!("{iap}/subscription/google"),
        &alice.token,
        json!({
            "purchase_token": google_token,
            "product_id": "sub-b",
            "expire_time": "2031-01-01T00:00:00Z",
        }),
        "record a Google subscription",
    )
    .await;
    assert_eq!(google_sub["transaction_id"], md5_hex(&google_token));
    assert_eq!(google_sub["expiration_time"], 1_924_992_000_i64);

    let expired_token = format!("e2e-sub-expired-{suffix}");
    let expired = api_post(
        client,
        format!("{iap}/subscription/apple"),
        &alice.token,
        json!({
            "receipt": expired_token,
            "product_id": "sub-expired",
            "expire_time": "2000-01-01T00:00:00Z",
        }),
        "record an expired subscription",
    )
    .await;
    assert_eq!(expired["state"], 3);
    assert_eq!(expired["expiration_time"], 946_684_800_i64);

    let open_ended = api_post(
        client,
        format!("{iap}/subscription/apple"),
        &alice.token,
        json!({ "receipt": format!("e2e-sub-open-{suffix}"), "product_id": "sub-open" }),
        "record a subscription without an expiry",
    )
    .await;
    assert_eq!(open_ended["state"], 1);
    assert!(open_ended.get("expiration_time").is_none());

    let huawei_token = format!("e2e-sub-huawei-{suffix}");
    let huawei_sub = api_post(
        client,
        format!("{iap}/subscription/huawei"),
        &alice.token,
        json!({
            "purchase_token": huawei_token,
            "product_id": "sub-h",
            "expire_time": "2030-06-01T00:00:00Z",
        }),
        "record a Huawei subscription",
    )
    .await;
    assert_eq!(huawei_sub["transaction_id"], md5_hex(&huawei_token));

    let active = api_get(
        client,
        with_query(&format!("{iap}/subscription"), &[("limit", "100")]),
        &alice.token,
        "active subscriptions",
    )
    .await;
    let products: Vec<&str> = active["subscriptions"]
        .as_array()
        .expect("subscriptions")
        .iter()
        .map(|row| row["product_id"].as_str().expect("product"))
        .collect();
    assert_eq!(products.len(), 3);
    assert!(products.contains(&"sub-a"));
    assert!(products.contains(&"sub-b"));
    assert!(products.contains(&"sub-h"));
    assert!(!products.contains(&"sub-expired"));
    assert!(!products.contains(&"sub-open"));

    let by_product = api_get(
        client,
        format!("{iap}/subscription/sub-a"),
        &alice.token,
        "a subscription by product",
    )
    .await;
    assert_eq!(by_product["transaction_id"], md5_hex(&apple_token));
    refusal(
        client
            .get(format!("{iap}/subscription/sub-absent"))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "an unknown subscription product",
    )
    .await;

    // Re-recording a token updates that subscription instead of adding one.
    let renewed = api_post(
        client,
        format!("{iap}/subscription/apple"),
        &alice.token,
        json!({
            "receipt": apple_token,
            "product_id": "sub-a2",
            "expire_time": "2032-01-01T00:00:00Z",
        }),
        "renew the Apple subscription",
    )
    .await;
    assert_eq!(renewed["transaction_id"], md5_hex(&apple_token));
    assert_eq!(renewed["product_id"], "sub-a2");
    assert_eq!(renewed["expiration_time"], 1_956_528_000_i64);
    refusal(
        client
            .get(format!("{iap}/subscription/sub-a"))
            .bearer_auth(&alice.token)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "the replaced product is gone",
    )
    .await;
    let renewed = api_get(
        client,
        format!("{iap}/subscription/sub-a2"),
        &alice.token,
        "the renewed subscription",
    )
    .await;
    assert_eq!(renewed["transaction_id"], md5_hex(&apple_token));
    let still_active = api_get(
        client,
        with_query(&format!("{iap}/subscription"), &[("limit", "100")]),
        &alice.token,
        "subscriptions after a renewal",
    )
    .await;
    assert_eq!(
        still_active["subscriptions"]
            .as_array()
            .expect("subscriptions")
            .len(),
        3
    );

    // Subscription paging uses the same offset cursor as purchases.
    let page = api_get(
        client,
        with_query(&format!("{iap}/subscription"), &[("limit", "2")]),
        &alice.token,
        "first subscription page",
    )
    .await;
    let cursor = page["next_cursor"]
        .as_str()
        .expect("a subscription cursor")
        .to_owned();
    assert_eq!(cursor, base64::engine::general_purpose::STANDARD.encode("2"));
    let page = api_get(
        client,
        with_query(
            &format!("{iap}/subscription"),
            &[("limit", "2"), ("cursor", &cursor)],
        ),
        &alice.token,
        "second subscription page",
    )
    .await;
    assert_eq!(
        page["subscriptions"].as_array().expect("subscriptions").len(),
        1
    );
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
