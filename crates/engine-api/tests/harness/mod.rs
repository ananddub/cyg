//! Shared Testcontainers harness for the gap suites (`gap_http.rs`, `gap_ws.rs`).
//!
//! This is the same plane `rest_e2e.rs` boots — Postgres 16 in a container, the
//! real router on a real socket — extracted so every e2e binary can drive the
//! identical, unmocked surface. It is `include!`-ed, so each test binary carries
//! its own copy; not every binary uses every helper, hence the allow.

#![allow(dead_code)]

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use engine_bus::Bus;
use engine_config::{ConsoleConfig, ControlConfig, GameConfig, SessionConfig};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};
use uuid::Uuid;

// Re-exported so each `#[path]`-included binary gets them with `use harness::*`.
pub use reqwest::{Client, StatusCode};
pub use serde_json::{Value, json};

pub const CONSOLE_USER: &str = "e2e-admin";
pub const CONSOLE_PASSWORD: &str = "e2e-password";
pub const SERVER_KEY: &str = "e2e-server-key";
pub const GAME_NAME: &str = "e2e-game";

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

pub struct Harness {
    pub base: String,
    pub port: u16,
    pub client: Client,
    pub pool: PgPool,
    /// The plane's own bus handle. Nothing in `engine-api` consumes
    /// `reloads_rx`, so a test can drain it and read the hot-swap requests the
    /// control plane published.
    pub bus: Arc<Bus>,
    shutdown: tokio::sync::oneshot::Sender<()>,
    server: tokio::task::JoinHandle<()>,
    container: ContainerAsync<Postgres>,
}

impl Harness {
    pub async fn start() -> Harness {
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
        let bus = Bus::new(64, 64);
        let state = engine_api::ApiState::new(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            &game,
            &control,
            bus.clone(),
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
            bus,
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

    pub async fn stop(self) {
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

pub async fn apply_schema(pool: &PgPool) {
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

pub async fn expect(resp: reqwest::Response, status: StatusCode, what: &str) -> String {
    let actual = resp.status();
    let body = resp.text().await.unwrap_or_default();
    assert_eq!(
        actual, status,
        "{what}: expected {status}, got {actual}: {body}"
    );
    body
}

pub async fn json_body(resp: reqwest::Response, status: StatusCode, what: &str) -> Value {
    let body = expect(resp, status, what).await;
    serde_json::from_str(&body)
        .unwrap_or_else(|err| panic!("{what}: response is not JSON ({err}): {body}"))
}

/// The `/v2` refusal envelope, checked for shape and message rather than only status.
pub async fn refusal(resp: reqwest::Response, status: StatusCode, what: &str) -> String {
    let body = json_body(resp, status, what).await;
    body["error"]["message"]
        .as_str()
        .unwrap_or_else(|| panic!("{what}: refusal has no message: {body}"))
        .to_owned()
}

pub async fn console_login(client: &Client, base: &str) -> String {
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
pub fn server_key_header() -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{SERVER_KEY}:"))
    )
}

/// An 8-character random lowercase suffix for e2e identifiers.
///
/// Letters only: the moderation filter is a context matcher that reads digit
/// runs as leetspeak, so identifiers that pass through it must not sample
/// digits. `device_player` additionally re-checks the assembled username,
/// because the boundary between prefix and suffix can itself form a match.
pub fn letters_suffix() -> String {
    let mut value = Uuid::new_v4().as_u128();
    let mut out = String::new();
    while out.len() < 8 {
        out.push((b'a' + (value % 26) as u8) as char);
        value /= 26;
    }
    out
}

/// The same rustrict call `moderation::is_clean` makes on the server.
pub fn accepts_username(candidate: &str) -> bool {
    use rustrict::{Censor, Type};
    !Censor::from_str(candidate)
        .with_censor_threshold(Type::INAPPROPRIATE)
        .analyze()
        .is(Type::INAPPROPRIATE)
}

/// The `user_id` claim of a session token, as the server would read it.
pub fn jwt_uid(token: &Value) -> String {
    let token = token.as_str().expect("session token");
    let payload = token.split('.').nth(1).expect("JWT payload segment");
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .expect("base64url JWT payload");
    let claims: Value = serde_json::from_slice(&decoded).expect("JWT claims JSON");
    claims["uid"].as_str().expect("uid claim").to_owned()
}

/// A signed-in test player: the token pair plus the identifiers the server wrote.
pub struct Player {
    pub token: String,
    pub refresh_token: String,
    pub uid: String,
    pub username: String,
    pub device_id: String,
}

/// A fresh device account through the real `/v2/account/authenticate/device`
/// route, with the Basic server key the plane was booted with.
///
/// `label` stays within seven characters: the built name is `e2e-<label>-<8>`, and
/// the route refuses anything over the server's twenty-character username ceiling.
pub async fn device_player(client: &Client, base: &str, label: &str) -> Player {
    let device_id = format!("e2e-{label}-{}", Uuid::new_v4());
    let username = (0..50)
        .map(|_| format!("e2e-{label}-{}", letters_suffix()))
        .find(|candidate| accepts_username(candidate))
        .unwrap_or_else(|| panic!("`e2e-{label}-<suffix>` never yields a clean username"));
    let body = json_body(
        client
            .post(format!(
                "{base}/v2/account/authenticate/device?username={username}&create=true"
            ))
            .header(reqwest::header::AUTHORIZATION, server_key_header())
            .json(&json!({ "id": device_id, "vars": { "platform": "e2e" } }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "device authenticate",
    )
    .await;
    assert_eq!(body["created"], true, "{label}: device account created");
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

/// A JSON value from a field the API carries as an encoded string.
pub fn json_string(value: &Value) -> Value {
    match value {
        Value::String(text) => serde_json::from_str(text).unwrap_or(Value::Null),
        other => other.clone(),
    }
}
