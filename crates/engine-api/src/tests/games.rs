//! The multi-game tenancy surface: `games` and the console CRUD that owns it.
//!
//! Three layers, deliberately ordered by what they need to run. The *pure* tests below
//! (the DDL mirror, the key shapes, the derived backend mode, the DTO whitelist, and
//! every route's credential and body validation) always run: the routes they exercise
//! answer 400/401/403/503 *before* they read a table, which is the same trick the rest
//! of this test tree uses. The `db_gated` tests need a live Postgres and are skipped
//! when none is configured — they call the service functions directly against a pool
//! installed into [`ApiState`].
//!
//! What the plan freezes and these tests hold: an engine decides its backend
//! (`unity`/`unreal`/`godot` relay, `web`/`custom` wasm) and no body may name that
//! decision; three credentials are minted on create (`gm_`, `cyg_sec_`, `cyg_pub_`) of
//! which only the secret is detail-only; and a game's config is the room-tuning
//! document, spelled either nested or flat, with the flat spelling winning.

use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

use crate::ApiState;
use crate::dto::console::games::{
    CreateGameRequest, ENGINE_TYPES, GAME_BACKEND_MODES, GAME_STATUSES, GameConfig,
    UpdateGameRequest,
};
use crate::error::{
    INVALID_ARGUMENT, NOT_FOUND, PERMISSION_DENIED, UNAUTHENTICATED, UNAVAILABLE,
};
use crate::infra::console::{self, ConsoleRole};
use crate::services::games::schema::{
    GAMES_KEY_INDEX_DDL, GAMES_TABLE_DDL, ROOM_MODULES_INDEX_DDL, ROOM_MODULES_TABLE_DDL,
};
use crate::services::games::service::{GamesService, backend_mode_for_engine};
use crate::services::games::{API_KEY_PREFIX, CLIENT_KEY_PREFIX, GAME_KEY_PREFIX};

use super::server::V2Server;
use super::v2_state;
use super::wasm_upload::empty_wasm;

/// A module's bytes, as `room_modules.binary_bytes` stores them: the smallest payload a
/// real uploader could have sent — a well formed but empty WASM module, built by the
/// sibling test's hand-assembler. The ABI gate is `wasm_upload`'s business; here it is
/// the thing being stored.
fn wasm_bytes() -> Vec<u8> {
    empty_wasm()
}

/// A name no operator typed and no other test owns: the fixture rows of this file are
/// all findable (and deletable) by it.
const TEST_PREFIX: &str = "engine-api-tests";

/// The committed schema, read at test time so a change to the SQL file is a change
/// this file notices.
const GAMES_SQL: &str = include_str!("../../../../db/manual/games.sql");

/// A console token for a role no `console_user` row has to hold: the role travels
/// inside the token, exactly as the existing console tests mint one.
fn token_for(role: ConsoleRole) -> String {
    format!(
        "Bearer {}",
        console::mint("", "games-role-probe", "games@console.local", role)
    )
}

fn admin_token() -> String {
    token_for(ConsoleRole::Admin)
}

// ---------------------------------------------------------------------------
// Pure: the DDL mirror. `db/manual/games.sql` is the source of truth and
// `services/games/schema.rs` restates it for a plane booting against an empty
// database. This test is what makes the two drift a CI failure instead of a
// runtime "column does not exist".
// ---------------------------------------------------------------------------

/// Whitespace collapsed and `--` comments gone: the shape both sides are compared in,
/// so indentation and prose are not schema changes.
fn normalize(sql: &str) -> String {
    let without_comments = sql
        .lines()
        .map(|line| match line.find("--") {
            Some(cut) => &line[..cut],
            None => line,
        })
        .collect::<Vec<_>>()
        .join(" ");
    without_comments
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The index of the `)` closing the parenthesised block opened at `open`.
fn matching_paren(text: &str, open: usize) -> usize {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut quoted = false;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] as char {
            '\'' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
        i += 1;
    }
    panic!("unbalanced parentheses in DDL: {text}");
}

/// One normalized item per column or table constraint of a `CREATE TABLE`, in the
/// order the file states them.
fn table_items(normalized: &str, table: &str) -> Vec<String> {
    let marker = format!("create table if not exists {table} (");
    let start = normalized
        .to_lowercase()
        .find(&marker)
        .unwrap_or_else(|| panic!("`{table}` is not created with `CREATE TABLE IF NOT EXISTS`"));
    let text = &normalized[start..];
    let open = text.find('(').expect("a CREATE TABLE opens its item list");
    let close = matching_paren(text, open);
    split_top_level(&text[open + 1..close])
}

/// Split a column list at the commas that are not inside a nested `(...)`, `[...]` or
/// a quoted literal — the JSONB default is exactly such a literal.
fn split_top_level(body: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut depth = 0i32;
    let mut quoted = false;
    let mut current = String::new();
    for ch in body.chars() {
        match ch {
            '\'' => {
                quoted = !quoted;
                current.push(ch);
            }
            '(' | '[' if !quoted => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' if !quoted => {
                depth -= 1;
                current.push(ch);
            }
            ',' if !quoted && depth == 0 => {
                items.push(collapse(&current));
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    items.push(collapse(&current));
    items.retain(|item| !item.is_empty());
    items
}

fn collapse(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_owned()
}

/// The statements of a script, split on the semicolons that end them.
fn statements(script: &str) -> Vec<String> {
    script
        .split(';')
        .map(collapse)
        .filter(|statement| !statement.is_empty())
        .collect()
}

/// The quoted JSON object that follows `keyword` in one column definition, parsed. A
/// `DEFAULT '{"a": 1}'::jsonb` is the only place a column carries a document, and the
/// only readable version of it is the literal — so the test reads the literal.
fn json_object_after(definition: &str, keyword: &str) -> Value {
    let after = definition
        .to_lowercase()
        .find(&keyword.to_lowercase())
        .unwrap_or_else(|| panic!("`{definition}` should carry a `{keyword}`"));
    let quoted = &definition[after + keyword.len()..];
    let open = quoted.find('{').expect("a JSONB default is a quoted object");
    let close = quoted[open..]
        .find('}')
        .expect("and it closes");
    serde_json::from_str(&quoted[open..open + close + 1])
        .unwrap_or_else(|err| panic!("the default document in `{definition}` parses: {err}"))
}

/// The tenancy tables are created identically in the committed SQL and in the runtime
/// bootstrap — column for column, constraint for constraint.
#[test]
fn the_runtime_ddl_mirrors_the_committed_games_sql() {
    let sql = normalize(GAMES_SQL);

    assert_eq!(
        table_items(&sql, "games"),
        table_items(&normalize(GAMES_TABLE_DDL), "games"),
        "games() drifted between db/manual/games.sql and GAMES_TABLE_DDL"
    );
    assert_eq!(
        table_items(&sql, "room_modules"),
        table_items(&normalize(ROOM_MODULES_TABLE_DDL), "room_modules"),
        "room_modules() drifted between db/manual/games.sql and ROOM_MODULES_TABLE_DDL"
    );

    // The parts of the schema a `CREATE TABLE` does not carry: the two indexes. Set
    // equality, so an index added to one side only is a failure either way.
    let mut from_sql: Vec<String> = statements(GAMES_SQL)
        .into_iter()
        .map(|statement| statement.to_lowercase())
        .filter(|statement| statement.starts_with("create index"))
        .collect();
    let mut from_rust: Vec<String> = [GAMES_KEY_INDEX_DDL, ROOM_MODULES_INDEX_DDL]
        .iter()
        .map(|ddl| normalize(ddl).to_lowercase())
        .collect();
    from_sql.sort();
    from_rust.sort();
    assert_eq!(from_sql, from_rust, "the tenancy indexes drifted");

    // Every column and constraint the console SQL promises, named, so a reviewer of a
    // failure sees the contract rather than a diff of tokens. The two credentials a
    // game is minted with beyond its tenant key — the secret and the publishable client
    // key — and the derived backend mode are in this list precisely because dropping
    // one of them from either side is a product change, not a refactor.
    let games = table_items(&sql, "games");
    for column in [
        "id uuid primary key default gen_random_uuid()",
        "game_key varchar(64) unique not null",
        "name varchar(128) not null",
        "api_key varchar(128) not null",
        "client_key varchar(128) not null",
        "engine_type varchar(32) not null default 'unity'",
        "backend_mode varchar(32) not null default 'relay'",
        "status varchar(32) not null default 'active'",
        "config jsonb not null",
        "created_at timestamptz not null default now()",
        "updated_at timestamptz not null default now()",
    ] {
        assert!(
            games
                .iter()
                .any(|item| item.to_lowercase() == column
                    || item.to_lowercase().starts_with(&format!("{column} "))),
            "`games` must keep its `{column}` column; found {games:?}"
        );
    }

    // The same columns on the runtime side, named, so a drift in the bootstrap says
    // which column went missing rather than printing two token lists.
    let runtime = table_items(&normalize(GAMES_TABLE_DDL), "games");
    for column in ["client_key", "backend_mode", "api_key", "config"] {
        assert!(
            runtime
                .iter()
                .any(|item| item.to_lowercase().starts_with(column)),
            "the runtime `games` bootstrap must declare `{column}`; found {runtime:?}"
        );
    }

    // The config default is the room-tuning document of the current plan: tick rate,
    // capacity, persistence and the idle close. All three spellings of it — the
    // column's own `DEFAULT`, the bootstrap's, and `GameConfig::default()` — have to
    // be the same object, and the retired keys of the old plan have to be in none of
    // them: a default that still spells `physics` is a game whose runtime reads a
    // setting no plane implements.
    let config = games
        .iter()
        .find(|item| item.to_lowercase().starts_with("config jsonb"))
        .expect("games has a config column");
    let column_default = json_object_after(config, "default");
    assert!(
        !column_default.to_string().contains("physics")
            && !column_default.to_string().contains("lag_compensation")
            && !column_default.to_string().contains("allow_storage"),
        "the retired config keys must not survive in the column default: {column_default}"
    );
    assert_eq!(
        column_default,
        serde_json::from_str::<Value>(crate::services::games::schema::DEFAULT_GAME_CONFIG_JSON)
            .expect("the restated default document parses"),
        "the column default and DEFAULT_GAME_CONFIG_JSON are one document"
    );
    assert_eq!(
        column_default,
        GameConfig::default().to_value(),
        "…and so is the typed config's own default"
    );

    let modules = table_items(&sql, "room_modules");
    for item in [
        "game_id uuid not null references games(id) on delete cascade",
        "version int not null",
        "binary_bytes bytea not null",
        "sha256 varchar(64) not null",
        "is_active boolean not null default false",
        "constraint room_modules_game_version unique (game_id, version)",
    ] {
        assert!(
            modules
                .iter()
                .any(|found| found.to_lowercase() == item),
            "`room_modules` must keep its `{item}`; found {modules:?}"
        );
    }
}

/// `init-oriole.sql` is the same schema for a fresh database: a game table that only
/// exists in one of the two files is a plane that boots with half a feature.
#[test]
fn init_oriole_and_the_manual_migration_agree() {
    const INIT_SQL: &str = include_str!("../../../../init-oriole.sql");
    let init = normalize(INIT_SQL);
    let manual = normalize(GAMES_SQL);

    for table in ["games", "room_modules"] {
        assert_eq!(
            table_items(&manual, table),
            table_items(&init, table),
            "`{table}` differs between db/manual/games.sql and init-oriole.sql"
        );
    }
}

// ---------------------------------------------------------------------------
// Pure: the shapes the contract freezes — key prefixes, the engine/status
// whitelists, and what a request body may and may not name.
// ---------------------------------------------------------------------------

/// A tenant key is `gm_…`, a secret `cyg_sec_…` and a publishable client key
/// `cyg_pub_…`, and all three are minted server-side. The prefix lengths are asserted
/// against the column widths the schema declares, so a longer prefix cannot silently
/// make a minted value too wide for `VARCHAR(128)` — or, for the tenant key, for
/// `VARCHAR(64)`.
#[test]
fn the_minted_credentials_keep_their_prefixes_and_fit_their_columns() {
    assert_eq!(GAME_KEY_PREFIX, "gm_");
    assert_eq!(API_KEY_PREFIX, "cyg_sec_");
    assert_eq!(CLIENT_KEY_PREFIX, "cyg_pub_");

    // The two `cyg_` credentials are the same shape with different prefixes: one is a
    // secret that only the detail view prints, the other ships inside the built client.
    assert_ne!(
        API_KEY_PREFIX, CLIENT_KEY_PREFIX,
        "a secret and a publishable key cannot share a prefix, or a log line cannot \
         tell which leaked"
    );

    // `game_key` is VARCHAR(64): the slug is capped at 50 characters by the minting
    // code, so the budget is prefix + slug + one '_' separator + a 6-hex suffix.
    const KEY_SLUG_BUDGET: usize = 50;
    const KEY_SUFFIX_LEN: usize = 6;
    assert!(
        GAME_KEY_PREFIX.len() + KEY_SLUG_BUDGET + 1 + KEY_SUFFIX_LEN <= 64,
        "a game key must fit the VARCHAR(64) column"
    );

    // Both credentials are a hex SHA-256 (64 characters) behind their prefix, and the
    // wider prefix of the two is the budget that matters.
    for prefix in [API_KEY_PREFIX, CLIENT_KEY_PREFIX] {
        assert!(
            prefix.len() + 64 <= 128,
            "a `{prefix}`-prefixed credential must fit its VARCHAR(128) column"
        );
    }
}

/// Five engines, two lifecycles, and the mode split the plan names: `unity`, `unreal`
/// and `godot` are relay games whose rooms live in the C# relay, so the WASM plane does
/// not apply to them; `web` and `custom` are authoritative and boot an uploaded module.
/// The derivation is a pure function of the engine, which is what makes the column
/// unforgeable — no body names a mode.
#[test]
fn the_engine_and_status_whitelists_are_the_frozen_ones() {
    let mut engines = ENGINE_TYPES.to_vec();
    engines.sort();
    engines.dedup();
    assert_eq!(
        engines,
        ["custom", "godot", "unity", "unreal", "web"],
        "the registrable engines are the five the plan ships"
    );
    for engine in ["unity", "web", "custom"] {
        assert!(
            ENGINE_TYPES.contains(&engine),
            "`{engine}` must stay a registrable engine"
        );
    }
    assert!(
        !ENGINE_TYPES.contains(&"phaser"),
        "the whitelist is a whitelist: an engine the plane cannot run is refused"
    );

    assert_eq!(GAME_STATUSES.as_slice(), ["active", "suspended", "archived"]);
    assert_eq!(GAME_BACKEND_MODES.as_slice(), ["relay", "wasm"]);
}

/// The whole mode table, engine by engine, and the invariant under it: a registrable
/// engine always derives a mode that is one of the two, and anything the whitelist does
/// not know relays — the safe direction, since a relay game never loads a module.
#[test]
fn an_engine_decides_its_backend_mode_and_only_web_and_custom_take_wasm() {
    assert_eq!(backend_mode_for_engine("unity"), "relay");
    assert_eq!(backend_mode_for_engine("unreal"), "relay");
    assert_eq!(backend_mode_for_engine("godot"), "relay");
    assert_eq!(backend_mode_for_engine("web"), "wasm");
    assert_eq!(backend_mode_for_engine("custom"), "wasm");

    for engine in ENGINE_TYPES {
        assert!(
            GAME_BACKEND_MODES.contains(&backend_mode_for_engine(engine)),
            "`{engine}` must derive a mode the console knows: {}",
            backend_mode_for_engine(engine)
        );
    }
    // The default engine of the schema relays: a create that names no engine is a
    // Unity game, and a Unity game never reaches the WASM plane.
    assert_eq!(backend_mode_for_engine("unity"), "relay");
}

/// The four keys of the room-tuning document, and nothing else: the retired
/// physics/lag-compensation/allow-storage trio was the old plan's, and a typed config
/// that still serializes it writes settings no runtime reads.
#[test]
fn the_game_config_document_is_the_room_tuning_of_the_current_plan() {
    let keys = {
        let mut keys: Vec<String> = GameConfig::default()
            .to_value()
            .as_object()
            .expect("a config serializes to an object")
            .keys()
            .cloned()
            .collect();
        keys.sort();
        keys
    };
    assert_eq!(
        keys,
        ["auto_close_seconds", "max_clients", "persistent_rooms", "tick_rate"],
        "the config's field list is the plan's four keys"
    );

    let defaults = GameConfig::default().to_value();
    assert_eq!(defaults["tick_rate"], json!(128));
    assert_eq!(defaults["max_clients"], json!(16));
    assert_eq!(defaults["persistent_rooms"], json!(false));
    assert_eq!(defaults["auto_close_seconds"], json!(30));

    // `#[serde(default)]` is what lets a partial document read as a complete one, so a
    // caller who names only a tick rate still gets a capacity and a close timeout.
    let partial: GameConfig = serde_json::from_value(json!({"tick_rate": 60}))
        .expect("a partial config deserializes");
    assert_eq!(partial.tick_rate, 60);
    assert_eq!(partial.max_clients, 16);
    assert_eq!(partial.auto_close_seconds, 30);
    assert!(!partial.persistent_rooms);
}

/// `CreateGameRequest` carries the name, the engine and the config in *both* of its
/// two spellings, and nothing else: a caller that could name its own `game_key`,
/// `client_key`, `api_key` or `backend_mode` could choose its own secret or put a Unity
/// relay game onto the WASM plane.
#[test]
fn a_create_request_takes_the_config_flat_or_nested_and_no_credential() {
    let bare: CreateGameRequest = serde_json::from_value(json!({"name": "Tank Arena"}))
        .expect("a name alone is a complete create");
    assert_eq!(bare.name, "Tank Arena");
    assert_eq!(bare.engine_type, None);
    assert_eq!(bare.config, None);

    let nested: CreateGameRequest = serde_json::from_value(json!({
        "name": "Tank Arena",
        "engine_type": "godot",
        "config": {"tick_rate": 64},
    }))
    .expect("the nested spelling deserializes");
    assert_eq!(nested.engine_type.as_deref(), Some("godot"));
    let config = nested.config.expect("a named config is kept as the caller wrote it");
    assert_eq!(config["tick_rate"], json!(64));

    // The flat spelling of the product plan, and every one of its fields is held.
    let flat: CreateGameRequest = serde_json::from_value(json!({
        "name": "Tank Arena",
        "engine_type": "web",
        "tick_rate": 60,
        "max_clients": 8,
        "persistent_rooms": true,
        "auto_close_seconds": 12,
    }))
    .expect("the flat spelling deserializes");
    let printed = format!("{flat:?}");
    assert_eq!(flat.engine_type.as_deref(), Some("web"));
    for (key, value) in [
        ("tick_rate", "60"),
        ("max_clients", "8"),
        ("persistent_rooms", "true"),
        ("auto_close_seconds", "12"),
    ] {
        assert!(
            printed.contains(key) && printed.contains(value),
            "a flat `{key}` has to be kept by the create, not dropped as an unknown \
             field: {printed}"
        );
    }

    // A client cannot smuggle a credential or a mode in: the shape has no field for
    // one, so the keys a hostile body names are dropped rather than stored.
    let hostile: CreateGameRequest = serde_json::from_value(json!({
        "name": "Tank Arena",
        "game_key": "gm_mine_000001",
        "api_key": "cyg_sec_preselected",
        "client_key": "cyg_pub_preselected",
        "backend_mode": "wasm",
    }))
    .expect("unknown fields are dropped, not an error");
    let printed = format!("{hostile:?}");
    for smuggled in ["gm_mine", "cyg_sec", "cyg_pub", "wasm"] {
        assert!(
            !printed.contains(smuggled),
            "a create must not be able to carry `{smuggled}`: {printed}"
        );
    }
    assert_eq!(hostile.name, "Tank Arena");
}

/// `UpdateGameRequest` is a patch: an absent field keeps its stored value, and no
/// patch shape offers a credential — nor `backend_mode`, which a patch to
/// `engine_type` recomputes server-side rather than accepting from the body.
#[test]
fn an_update_request_is_a_patch_over_optional_fields() {
    let empty: UpdateGameRequest =
        serde_json::from_value(json!({})).expect("every patch field is optional");
    assert_eq!(empty.name, None);
    assert_eq!(empty.engine_type, None);
    assert_eq!(empty.status, None);
    assert_eq!(empty.config, None);

    let renamed: UpdateGameRequest =
        serde_json::from_value(json!({"name": "Second Name"})).expect("a name-only patch");
    assert_eq!(renamed.name.as_deref(), Some("Second Name"));
    assert_eq!(renamed.status, None, "an absent status is not a status change");

    let hostile: UpdateGameRequest = serde_json::from_value(json!({
        "name": "Second Name",
        "backend_mode": "wasm",
        "api_key": "cyg_sec_rotated_by_me",
        "client_key": "cyg_pub_rotated_by_me",
        "game_key": "gm_renamed",
    }))
    .expect("a patch with fields the shape has no room for is still a patch");
    let printed = format!("{hostile:?}");
    for smuggled in ["wasm", "cyg_sec", "cyg_pub", "gm_renamed"] {
        assert!(
            !printed.contains(smuggled),
            "a patch cannot carry `{smuggled}`; the mode is derived and the \
             credentials are minted: {printed}"
        );
    }
    assert_eq!(hostile.name.as_deref(), Some("Second Name"));
}

// ---------------------------------------------------------------------------
// Pure (no database): the routes are registered, gated, and validate their body
// before they reach for a pool.
// ---------------------------------------------------------------------------

/// The games CRUD paths, with the method each one answers.
fn games_routes() -> Vec<(&'static str, String)> {
    let id = "00000000-0000-0000-0000-000000000000";
    vec![
        ("GET", "/v2/console/games".to_owned()),
        ("POST", "/v2/console/games".to_owned()),
        ("GET", format!("/v2/console/games/{id}")),
        ("PUT", format!("/v2/console/games/{id}")),
        ("DELETE", format!("/v2/console/games/{id}")),
    ]
}

/// The generated Unity server-settings companion, in each of its two spellings: the
/// default is the ScriptableObject asset a build drops into `Assets/`, and
/// `?format=json` is the connection document for a client that assembles its own.
fn unity_settings_routes() -> Vec<(&'static str, String)> {
    let id = "00000000-0000-0000-0000-000000000000";
    vec![
        ("GET", format!("/v2/console/games/{id}/unity-settings")),
        ("GET", format!("/v2/console/games/{id}/unity-settings?format=asset")),
        ("GET", format!("/v2/console/games/{id}/unity-settings?format=json")),
    ]
}

/// Every games route is a console route: no credential, no answer — and the refusal is
/// the console realm's own wording. The settings download is on that list because a
/// generated asset is a credential carrier: it is handed to a signed-in operator or to
/// nobody.
#[test]
fn every_games_route_demands_a_console_token() {
    let server = V2Server::start(v2_state());
    for (method, path) in games_routes()
        .into_iter()
        .chain(unity_settings_routes())
    {
        let reply = server.send(&method, &path, None, "{}");
        assert_eq!(
            reply.refusal(UNAUTHENTICATED),
            "Console token required",
            "{method} {path}"
        );
    }
}

/// Registering and retiring a game are admin-only; a patch is any write role; reading
/// the list is every console user, including the read-only one.
#[test]
fn the_games_writes_are_gated_on_the_admin_and_write_roles() {
    let server = V2Server::start(v2_state());
    let id = Uuid::new_v4();

    // Creating a tenant asks for more than write access, so even that refusal is the
    // admin one — the role gate runs before the body is ever validated.
    for role in [ConsoleRole::Readonly, ConsoleRole::Developer] {
        let reply = server.post(
            "/v2/console/games",
            Some(&token_for(role)),
            r#"{"name":"Tank Arena"}"#,
        );
        assert_eq!(
            reply.refusal(PERMISSION_DENIED),
            "This console action requires the admin role.",
            "a create from {role:?}"
        );
    }
    let reply = server.delete(
        &format!("/v2/console/games/{id}"),
        Some(&token_for(ConsoleRole::Developer)),
    );
    assert_eq!(
        reply.refusal(PERMISSION_DENIED),
        "This console action requires the admin role."
    );

    // A patch is a write rather than an admin action: readonly is refused, a developer
    // gets as far as the missing database.
    let reply = server.put(
        &format!("/v2/console/games/{id}"),
        Some(&token_for(ConsoleRole::Readonly)),
        r#"{"name":"Renamed"}"#,
    );
    assert_eq!(
        reply.refusal(PERMISSION_DENIED),
        "This console user has read-only access."
    );
    let reply = server.put(
        &format!("/v2/console/games/{id}"),
        Some(&token_for(ConsoleRole::Developer)),
        r#"{"name":"Renamed"}"#,
    );
    assert_eq!(reply.status, 503, "{}", reply.body);

    // Reading the list and one game is open to the least-privileged console user.
    for path in [
        "/v2/console/games".to_owned(),
        format!("/v2/console/games/{id}"),
    ] {
        let reply = server.get(&path, Some(&token_for(ConsoleRole::Readonly)));
        assert_eq!(reply.status, 503, "{path}: {}", reply.body);
    }
}

/// The settings download is a read, so every console role reaches it — and each of its
/// two formats is mounted, since a dashboard that asks for `?format=json` and gets a
/// 404 has no way to learn the asset is what it received by default.
#[test]
fn the_unity_settings_route_is_a_read_mounted_in_both_formats() {
    let server = V2Server::start(v2_state());
    let id = Uuid::new_v4();

    for role in [ConsoleRole::Readonly, ConsoleRole::Developer, ConsoleRole::Admin] {
        for path in [
            format!("/v2/console/games/{id}/unity-settings"),
            format!("/v2/console/games/{id}/unity-settings?format=asset"),
            format!("/v2/console/games/{id}/unity-settings?format=json"),
        ] {
            let reply = server.get(&path, Some(&token_for(role)));
            assert_ne!(reply.status, 404, "`{role:?}` cannot reach {path}");
            assert_ne!(
                reply.status, 405,
                "{path} is mounted but does not claim GET: {}",
                reply.body
            );
            // Nothing behind this plane's handler is a table it has not reached, and a
            // settings document is a thing only a game row can produce.
            assert_eq!(
                reply.refusal(UNAVAILABLE),
                "The database is not configured, set [control.session] database_url.",
                "{role:?} GET {path}"
            );
        }
    }
}

/// The name, engine and config checks answer *before* the pool is touched, which is why
/// they are assertable on a plane with no database: the refusal is the handler's own
/// wording, and a valid request is stopped by the missing database instead.
#[test]
fn the_create_body_is_validated_before_the_database_is_reached() {
    let server = V2Server::start(v2_state());
    let auth = admin_token();

    let reply = server.post("/v2/console/games", Some(&auth), r#"{"name":""}"#);
    assert_eq!(reply.refusal(INVALID_ARGUMENT), "A game needs a name.");

    let reply = server.post("/v2/console/games", Some(&auth), "{}");
    assert_eq!(
        reply.refusal(INVALID_ARGUMENT),
        "A game needs a name.",
        "a create that names no field is refused for the field that matters"
    );

    let reply = server.post(
        "/v2/console/games",
        Some(&auth),
        r#"{"name":"Tank Arena","engine_type":"phaser"}"#,
    );
    let message = reply.refusal(INVALID_ARGUMENT);
    assert!(
        message.contains("Unknown engine type 'phaser'"),
        "{message}"
    );
    for engine in ENGINE_TYPES {
        assert!(
            message.contains(engine),
            "an engine refusal names the whole whitelist, {engine} missing from {message}"
        );
    }

    let reply = server.post(
        "/v2/console/games",
        Some(&auth),
        r#"{"name":"Tank Arena","config":[1,2,3]}"#,
    );
    assert_eq!(
        reply.refusal(INVALID_ARGUMENT),
        "A game config must be a JSON object."
    );

    // Well formed, and stopped only by the absent database — the sound of a request
    // that passed every check in front of it. Both spellings of the config, and the
    // fifth engine: `custom` is authoritative, so a create naming it is as acceptable
    // as one naming `unity`.
    for body in [
        r#"{"name":"Tank Arena","engine_type":"unity"}"#,
        r#"{"name":"Tank Arena","engine_type":"custom","tick_rate":60,"max_clients":8,
           "persistent_rooms":true,"auto_close_seconds":12}"#,
        r#"{"name":"Tank Arena","engine_type":"web","config":{"tick_rate":30}}"#,
        // A body that tries to name its own mode and credentials is not refused for it:
        // the fields are simply not part of the shape, so they are dropped and the
        // create is the valid request it always was.
        r#"{"name":"Tank Arena","engine_type":"unity","backend_mode":"wasm",
           "api_key":"cyg_sec_preselected"}"#,
    ] {
        let reply = server.post("/v2/console/games", Some(&auth), &collapse(body));
        assert_eq!(
            reply.refusal(UNAVAILABLE),
            "The database is not configured, set [control.session] database_url.",
            "a well formed create {body} is stopped by the missing database, not by \
             its body"
        );
    }
}

/// An id that is not a UUID is refused as the game it names, with no database needed.
#[test]
fn a_malformed_game_id_is_a_bad_request_naming_the_game() {
    let server = V2Server::start(v2_state());
    let auth = admin_token();
    for (method, path) in [
        ("GET", "/v2/console/games/not-a-uuid".to_owned()),
        ("PUT", "/v2/console/games/not-a-uuid".to_owned()),
        ("DELETE", "/v2/console/games/not-a-uuid".to_owned()),
        ("GET", "/v2/console/games/not-a-uuid/unity-settings".to_owned()),
        (
            "GET",
            "/v2/console/games/not-a-uuid/unity-settings?format=json".to_owned(),
        ),
    ] {
        let reply = server.send(&method, &path, Some(&auth), "{}");
        assert_eq!(
            reply.refusal(INVALID_ARGUMENT),
            "Invalid game id.",
            "{method} {path}"
        );
    }
}

/// A game update that changes nothing is refused rather than silently bumping
/// `updated_at` — and the refusal is the handler's, not the database's.
#[test]
fn an_empty_patch_is_refused_before_the_database() {
    let server = V2Server::start(v2_state());
    let reply = server.put(
        &format!("/v2/console/games/{}", Uuid::new_v4()),
        Some(&admin_token()),
        "{}",
    );
    assert_eq!(
        reply.refusal(INVALID_ARGUMENT),
        "A game update has to change at least one field."
    );
}

/// An unknown status is refused by name, the same way an unknown engine is.
#[test]
fn an_unknown_game_status_is_refused_before_the_database() {
    let server = V2Server::start(v2_state());
    let reply = server.put(
        &format!("/v2/console/games/{}", Uuid::new_v4()),
        Some(&admin_token()),
        r#"{"status":"deleted"}"#,
    );
    let message = reply.refusal(INVALID_ARGUMENT);
    assert!(message.contains("Unknown game status 'deleted'"), "{message}");
    for status in GAME_STATUSES {
        assert!(message.contains(status), "{status} missing from {message}");
    }
}

/// A name over the column's 128 characters is refused instead of truncated by
/// Postgres, which is the difference between a bad request and a half-stored game.
#[test]
fn an_overlong_game_name_is_refused() {
    let server = V2Server::start(v2_state());
    let name = "n".repeat(129);
    let reply = server.post(
        "/v2/console/games",
        Some(&admin_token()),
        &format!(r#"{{"name":"{name}"}}"#),
    );
    assert_eq!(
        reply.refusal(INVALID_ARGUMENT),
        "A game name is at most 128 characters."
    );
}

// ---------------------------------------------------------------------------
// Pure: the generated Unity asset. The asset is the product's answer to "how does a
// built client learn where to connect", and its contract is the SDK class: Unity
// deserializes a `ScriptableObject` by field name and silently drops any key the class
// does not declare. So the field list is read from the C# source and from the template
// that emits the YAML, the way the WASM ABI list is read from the host — two files that
// have to agree, neither of them a copy of the other's prose.
// ---------------------------------------------------------------------------

/// The SDK class the asset binds to.
const UNITY_SDK_SOURCE: &str =
    include_str!("../../../../sdk/unity/Runtime/Core/CYGServerSettings.cs");

/// The controller that owns the asset's YAML template and its attachment name.
const GAMES_CONTROLLER_SOURCE: &str = include_str!("../controllers/console/games.rs");

/// The `[SerializeField]` fields of the SDK class — the only names Unity will read out
/// of an asset. Order is the declaration order, which is also the order a reviewer of a
/// generated asset expects to see.
fn sdk_serialized_fields() -> Vec<String> {
    UNITY_SDK_SOURCE
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("[SerializeField]")?;
            // `[SerializeField] ushort protocolVersion = 1;` — the type is the first
            // word and the field name the second.
            let name = rest.split_whitespace().nth(1)?.trim_end_matches(';');
            (!name.is_empty()
                && name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
                .then(|| name.to_owned())
        })
        .collect()
}

/// The YAML keys of the asset template: every two-space-indented `key:` line of the
/// `unity_asset_yaml` body. Unity's own header fields are all `m_`-prefixed, so the
/// underscore is what separates them from the class's serialized fields — and the
/// generator's `{…}` substitutions are irrelevant here, only the names they ride on.
fn asset_template_fields() -> Vec<String> {
    asset_template_body()
        .lines()
        .filter(|line| line.starts_with("  ") && !line.starts_with("   "))
        .filter_map(|line| line[2..].split_once(':'))
        .map(|(key, _value)| key.trim().to_owned())
        .filter(|key| !key.is_empty() && !key.contains('_') && key.contains(char::is_alphanumeric))
        .collect()
}

/// The generator's own text: everything from `fn unity_asset_yaml` to the brace that
/// closes it. Read from the file that owns it, the way the WASM ABI list is read from
/// the host's declaration — the two have to agree, and neither is a copy of the other's
/// prose.
fn asset_template_body() -> String {
    let start = GAMES_CONTROLLER_SOURCE
        .find("fn unity_asset_yaml")
        .expect("the controller owns the asset template");
    GAMES_CONTROLLER_SOURCE[start..]
        .lines()
        .take_while(|line| !line.starts_with('}'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The asset's serialized names are the SDK class's fields, exactly: a key the class
/// does not declare is dropped by Unity, and a field the class declares that the asset
/// omits is left at the class's own default — a client silently connecting somewhere
/// other than the game the operator registered.
#[test]
fn the_asset_template_serializes_exactly_the_fields_the_sdk_class_declares() {
    let declared = sdk_serialized_fields();
    assert_eq!(
        declared,
        [
            "serverHttpUrl".to_owned(),
            "serverWsPath".to_owned(),
            "roomName".to_owned(),
            "maxPlayers".to_owned(),
            "protocolVersion".to_owned(),
            "matchmakeMethod".to_owned(),
        ],
        "the SDK's serialized fields are the six the asset has to carry"
    );

    let emitted = asset_template_fields();
    for name in &declared {
        assert!(
            emitted.iter().any(|field| field == name),
            "the asset template must write `{name}`, which the SDK class declares; \
             it writes {emitted:?}"
        );
    }
    for field in &emitted {
        assert!(
            declared.iter().any(|name| name == field),
            "the asset template writes `{field}`, which `CYGServerSettings` does not \
             declare — Unity drops it: {emitted:?}"
        );
    }
    assert_eq!(
        emitted.len(),
        declared.len(),
        "and the two lists are the same length: no field is spelled twice"
    );
}

/// The header a Unity asset needs before any of its fields mean anything: the YAML and
/// TAG directives, the `!u!114` MonoBehaviour document, a script reference, and the
/// class's own name. Without them the file is a text document, not a ScriptableObject,
/// and `protocolVersion` is asserted as the number the SDK declares it to be — a string
/// there is a deserialization that silently lands on zero.
#[test]
fn the_asset_template_is_a_scriptable_object_document_of_the_sdk_class() {
    let template = asset_template_body();

    for marker in [
        "%YAML 1.1",
        "%TAG !u! tag:unity3d.com,2011:",
        "--- !u!114 &11400000",
        "MonoBehaviour:",
        "m_Script:",
        "m_Name: CYGServerSettings",
    ] {
        assert!(
            template.contains(marker),
            "a Unity asset opens with `{marker}`; the template does not emit it"
        );
    }

    // `protocolVersion` is a `ushort` on the class and a `u16` in the generator, and the
    // value it substitutes is a literal number in the template — never a name.
    let line = template
        .lines()
        .find(|line| line.trim_start().starts_with("protocolVersion:"))
        .expect("the template writes protocolVersion");
    let value = line.split_once(':').expect("a key and a value").1.trim();
    assert!(
        value.starts_with('{') && value.ends_with('}'),
        "`{value}` is substituted from the generator's own constant"
    );
    let constant = value.trim_matches('{').trim_matches('}').trim();
    let declaration = format!("const {constant}: u16");
    assert!(
        GAMES_CONTROLLER_SOURCE.contains(&declaration),
        "protocol version must be a numeric constant, declared as `{declaration}`"
    );
    let numeric: u32 = GAMES_CONTROLLER_SOURCE
        .lines()
        .find(|line| line.trim_start().starts_with(&declaration))
        .and_then(|line| line.split_once('=')?.1.trim().trim_end_matches(';').parse().ok())
        .expect("and its value is a number, not an expression");
    assert!(
        numeric <= u32::from(u16::MAX),
        "and it fits the `ushort` the SDK field is: {numeric}"
    );

    // The attachment name the console promises the operator, and the plan's own default
    // for it, come from the same file that emits the body. The controller writes it as a
    // Rust string literal, so the inner quotes arrive on disk escaped (`\"`); matching on
    // the quote-free markers keeps this robust to how that literal is escaped rather than
    // pinning a byte sequence the source formatter is free to change.
    let names_the_asset = GAMES_CONTROLLER_SOURCE.contains("attachment")
        && GAMES_CONTROLLER_SOURCE.contains("filename=")
        && GAMES_CONTROLLER_SOURCE.contains("CYGServerSettings.asset");
    assert!(
        names_the_asset,
        "the asset must arrive as `CYGServerSettings.asset`, or a build drops it into \
         the project under a name the SDK does not expect"
    );
}

/// The defaults a generated asset falls back to are the SDK's own constants, and the
/// SDK is the file that decides them: `cyg_room` for the room a client joins when it
/// names none, `joinOrCreate` for the method it calls with, and `1` for the protocol
/// revision. The connection document the console prints beside the asset has to carry
/// the same three, because a dashboard that shows `defaultRoom: "lobby"` teaches an
/// operator a room the built game will never ask for.
#[test]
fn the_settings_defaults_are_the_ones_the_sdk_declares() {
    for default in [
        ("DefaultRoomName", "\"cyg_room\""),
        ("DefaultMatchmakeMethod", "\"joinOrCreate\""),
        ("protocolVersion", "1"),
    ] {
        let (name, value) = default;
        assert!(
            UNITY_SDK_SOURCE.contains(&format!("{name} = {value}")),
            "the SDK class declares `{name} = {value}`; if that changed, so did the \
             asset this file checks"
        );
    }
    // The generator restates them rather than inventing them, in the file that owns the
    // connection document.
    let service = include_str!("../services/games/service.rs");
    assert!(
        service.contains("DEFAULT_ROOM_NAME: &str = \"cyg_room\"")
            && service.contains("SERVER_WS_PATH: &str = \"/ws\""),
        "the service restates the SDK's room default and the plane's `/ws` mount"
    );
}

// ---------------------------------------------------------------------------
// Database-gated integration. Skipped (with a printed reason) unless a DSN is
// configured, the same two names `engine-config` reads. These helpers are shared with
// `wasm_upload`, whose upload lifecycle writes `room_modules` rows for a game created
// here — and which now has to be a `wasm` game for the mode guard to let it through.
// ---------------------------------------------------------------------------

pub(crate) mod db {
    use super::*;

    /// The DSN the plane would boot with, if the harness was given one.
    pub(crate) fn url() -> Option<String> {
        ["OX_DATABASE_URL", "DATABASE_URL"]
            .iter()
            .find_map(|name| std::env::var(name).ok())
            .filter(|value| !value.is_empty())
    }

    /// A plane with a real pool installed, plus that pool for the raw reads these
    /// tests make alongside the service. `None` means "no Postgres here" and the
    /// caller prints the skip and returns.
    pub(crate) async fn state() -> Option<(ApiState, sqlx::PgPool)> {
        let dsn = url()?;
        let pool = match sqlx::postgres::PgPoolOptions::new()
            .max_connections(4)
            .acquire_timeout(std::time::Duration::from_secs(3))
            .connect(&dsn)
            .await
        {
            Ok(pool) => pool,
            Err(err) => {
                eprintln!("skipping: `{dsn}` is not a reachable Postgres ({err})");
                return None;
            }
        };
        // One bootstrap per test binary: `CREATE TABLE IF NOT EXISTS` run by several
        // sessions at the same instant can lose to itself on the catalog, and the
        // statements are idempotent anyway — `ensure_schema_creates_the_tables_and_is_idempotent`
        // is the test that runs them on purpose, twice, in sequence.
        static BOOTSTRAP: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();
        BOOTSTRAP
            .get_or_init(|| async {
                crate::services::games::schema::ensure_schema(&pool)
                    .await
                    .expect("the tenancy tables must be creatable on the test database");
            })
            .await;

        let mut state = v2_state();
        state.q = Some(std::sync::Arc::new(crate::repositories::db::Queries::new(
            pool.clone(),
        )));
        state.db = Some(pool.clone());
        Some((state, pool))
    }

    /// A game name no other test or operator owns, and the delete that takes its
    /// modules with it (`ON DELETE CASCADE`).
    pub(crate) fn fixture_name(label: &str) -> String {
        format!("{TEST_PREFIX} {label} {}", Uuid::new_v4())
    }

    /// A fixture as the console would create it: only a name, so every derived field —
    /// the engine, the mode, the config — is the server's answer. A Unity relay game,
    /// which is what a WASM route refuses.
    pub(crate) async fn create(state: &ApiState, label: &str) -> crate::dto::console::games::GameDto {
        create_engine(state, label, None).await
    }

    /// The same, with an engine named, because the engine is what decides the backend
    /// mode — and every module-lifecycle fixture in `wasm_upload` needs a `wasm` game
    /// for the guard to let it through at all.
    pub(crate) async fn create_engine(
        state: &ApiState,
        label: &str,
        engine: Option<&str>,
    ) -> crate::dto::console::games::GameDto {
        let body = CreateGameRequest {
            name: fixture_name(label),
            engine_type: engine.map(str::to_owned),
            ..Default::default()
        };
        GamesService::create_game(state, &body)
            .await
            .expect("a named game registers")
    }

    /// A game whose rooms an uploaded module runs: `custom` is the authoritative
    /// hand-rolled engine, so its mode is `wasm` and the module plane serves it.
    pub(crate) async fn create_wasm(
        state: &ApiState,
        label: &str,
    ) -> crate::dto::console::games::GameDto {
        create_engine(state, label, Some("custom")).await
    }

    /// A create from a raw JSON body, so a test can spell the request exactly as an
    /// operator's console does — flat fields, nested config, or both at once — without
    /// this file naming every field of the request shape.
    pub(crate) async fn create_from_value(
        state: &ApiState,
        body: Value,
    ) -> crate::dto::console::games::GameDto {
        let request: CreateGameRequest =
            serde_json::from_value(body).expect("the fixture body is a create request");
        GamesService::create_game(state, &request)
            .await
            .expect("the fixture body registers a game")
    }

    pub(crate) async fn forget(pool: &sqlx::PgPool, game_id: Uuid) {
        sqlx::query("DELETE FROM games WHERE id = $1")
            .bind(game_id)
            .execute(pool)
            .await
            .expect("the fixture game is deletable");
    }
}

/// A skipped database test says so, because a green run that silently tested nothing
/// is the failure mode worth narrating.
fn skipped_without_database() {
    eprintln!(
        "skipping: no Postgres configured — set OX_DATABASE_URL or DATABASE_URL to run the \
         games integration tests"
    );
}

/// `ensure_schema` is the runtime bootstrap of a plane pointed at an empty database, so
/// it has to be safe to run on every boot and on a database that already has the tables.
#[tokio::test]
async fn ensure_schema_creates_the_tables_and_is_idempotent() {
    let Some((_state, pool)) = db::state().await else {
        return skipped_without_database();
    };

    crate::services::games::schema::ensure_schema(&pool)
        .await
        .expect("a second ensure_schema changes nothing and reports no error");
    crate::services::games::schema::ensure_schema(&pool)
        .await
        .expect("and a third, for good measure");

    for table in ["games", "room_modules"] {
        let row = sqlx::query(
            "SELECT COUNT(*) AS n FROM information_schema.tables WHERE table_name = $1",
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .expect("the table listing is readable");
        assert_eq!(row.get::<i64, _>("n"), 1, "`{table}` must exist");
    }
}

/// The minted credentials are the contract's three — tenant key, secret, publishable
/// client key — and a create that names no config gets the complete default document of
/// the current plan rather than a hole.
#[tokio::test]
async fn create_game_mints_the_prefixed_keys_and_defaults_the_config() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };

    let game = db::create(&state, "keys").await;
    assert!(
        game.game_key.starts_with(GAME_KEY_PREFIX),
        "a tenant key is minted with the `{GAME_KEY_PREFIX}` prefix: {}",
        game.game_key
    );
    assert!(
        game.game_key.len() > GAME_KEY_PREFIX.len(),
        "and it is a prefix of something, not the whole key"
    );
    assert_eq!(game.status, "active", "a new game is live");
    assert_eq!(
        game.engine_type, "unity",
        "an unspecified engine is the schema's own default"
    );
    assert_eq!(
        game.backend_mode, "relay",
        "a Unity game is served by the C# relay, and that is not the caller's to decide"
    );
    assert_eq!(game.config["tick_rate"], json!(128), "{}", game.config);
    assert_eq!(game.config["max_clients"], json!(16));
    assert_eq!(game.config["persistent_rooms"], json!(false));
    assert_eq!(game.config["auto_close_seconds"], json!(30));
    for retired in ["physics", "lag_compensation", "allow_storage"] {
        assert!(
            game.config.get(retired).is_none(),
            "the retired `{retired}` key must not be in a fresh config: {}",
            game.config
        );
    }

    // The list reply is publishable-only: the client key rides on it, the secret does
    // not — which is why the detail view is the one call that has to be a deliberate
    // action.
    assert!(
        game.client_key.starts_with(CLIENT_KEY_PREFIX),
        "the publishable client key is minted with the `{CLIENT_KEY_PREFIX}` prefix: {}",
        game.client_key
    );
    assert_eq!(
        serde_json::to_value(&game)
            .expect("a create reply serializes")
            .get("api_key"),
        None,
        "and no write reply carries the secret"
    );

    let detail = GamesService::get_game(&state, game.id)
        .await
        .expect("the game just created is readable");
    assert!(
        detail.api_key.starts_with(API_KEY_PREFIX),
        "the secret carries the `{API_KEY_PREFIX}` prefix"
    );
    assert!(
        detail.api_key.len() <= 128,
        "and fits its VARCHAR(128) column: {}",
        detail.api_key.len()
    );
    assert_eq!(detail.game_key, game.game_key);
    assert_eq!(detail.client_key, game.client_key);
    assert_eq!(detail.backend_mode, game.backend_mode);

    // The three credentials are three values: a game whose public key were also its
    // secret would authenticate a client that read its own binary.
    assert_ne!(detail.api_key, detail.client_key);
    assert_ne!(detail.client_key, detail.game_key);
    assert!(detail.client_key.len() > CLIENT_KEY_PREFIX.len());

    // A caller-named engine is lowercased onto the whitelist, and a caller-named config
    // is kept whole rather than merged onto the defaults.
    let body = CreateGameRequest {
        name: db::fixture_name("explicit"),
        engine_type: Some("GODOT".to_owned()),
        config: Some(json!({"tick_rate": 30, "custom": "kept"})),
        ..Default::default()
    };
    let explicit = GamesService::create_game(&state, &body)
        .await
        .expect("an explicit engine and config register");
    assert_eq!(explicit.engine_type, "godot", "the whitelist is matched case-folded");
    assert_eq!(explicit.config["tick_rate"], json!(30));
    assert_eq!(explicit.config["custom"], json!("kept"));
    assert_eq!(
        explicit.config.get("max_clients"),
        None,
        "a named config replaces the defaults instead of being merged into them"
    );

    db::forget(&pool, game.id).await;
    db::forget(&pool, explicit.id).await;
}

/// The mode is a function of the engine and nothing else, and every registrable engine
/// produces a game whose stored row says which plane runs its rooms. A relay game is the
/// one the WASM plane refuses; this is where that distinction is created.
#[tokio::test]
async fn every_engine_derives_the_backend_mode_the_plan_assigns_it() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };

    for (engine, mode) in [
        ("unity", "relay"),
        ("unreal", "relay"),
        ("godot", "relay"),
        ("web", "wasm"),
        ("custom", "wasm"),
    ] {
        let game = db::create_engine(&state, &format!("mode-{engine}"), Some(engine)).await;
        assert_eq!(game.engine_type, engine);
        assert_eq!(
            game.backend_mode, mode,
            "`{engine}` must register on the `{mode}` backend"
        );
        let detail = GamesService::get_game(&state, game.id)
            .await
            .expect("the row is readable");
        assert_eq!(detail.backend_mode, mode, "and the row says so");
        db::forget(&pool, game.id).await;
    }
}

/// A patch that moves a game between the two families moves its mode with it, in both
/// directions: a Unity game that becomes a custom authoritative one is exactly the game
/// the module plane may now serve, and the way back has to work too.
#[tokio::test]
async fn switching_engine_type_flips_the_derived_backend_mode_both_ways() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };

    let game = db::create_engine(&state, "mode-flip", Some("unity")).await;
    assert_eq!(game.backend_mode, "relay");

    let to_wasm: UpdateGameRequest =
        serde_json::from_value(json!({"engine_type": "custom"})).expect("an engine patch");
    let patched = GamesService::update_game(&state, game.id, &to_wasm)
        .await
        .expect("a Unity game can become an authoritative one");
    assert_eq!(patched.engine_type, "custom");
    assert_eq!(
        patched.backend_mode, "wasm",
        "and its mode follows the engine, because no body named a mode"
    );

    let back: UpdateGameRequest =
        serde_json::from_value(json!({"engine_type": "godot"})).expect("a second engine patch");
    let patched = GamesService::update_game(&state, game.id, &back)
        .await
        .expect("and the way back is the same patch");
    assert_eq!(patched.engine_type, "godot");
    assert_eq!(patched.backend_mode, "relay");

    // A patch that names no engine leaves the mode alone: the mode is derived from the
    // engine, so a rename is not a plane change.
    let renamed: UpdateGameRequest =
        serde_json::from_value(json!({"name": "Renamed Relay"})).expect("a name patch");
    let patched = GamesService::update_game(&state, game.id, &renamed)
        .await
        .expect("a rename is applied");
    assert_eq!(patched.backend_mode, "relay", "a rename keeps the mode");
    assert_eq!(patched.engine_type, "godot");

    db::forget(&pool, game.id).await;
}

/// The flat spelling of the plan's create body wins over the same key inside `config`,
/// and a flat-only body still reads as a complete config: precedence asserted at the
/// service, because a deserializer that drops the flat fields or lets the nested copy
/// win is indistinguishable on the wire of a single request.
#[tokio::test]
async fn a_flat_config_field_overrides_the_same_key_spelled_nested() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };

    let both = db::create_from_value(
        &state,
        json!({
            "name": db::fixture_name("precedence"),
            "engine_type": "web",
            "tick_rate": 60,
            "auto_close_seconds": 12,
            "config": {"tick_rate": 20, "max_clients": 4, "auto_close_seconds": 99},
        }),
    )
    .await;
    assert_eq!(
        both.config["tick_rate"],
        json!(60),
        "the flat field beats the nested duplicate: {}",
        both.config
    );
    assert_eq!(
        both.config["auto_close_seconds"],
        json!(12),
        "for every key the flat body names: {}",
        both.config
    );
    assert_eq!(
        both.config["max_clients"],
        json!(4),
        "and a key only the nested document names is kept as written: {}",
        both.config
    );

    // Flat only: the keys the body does not name are not invented — a caller that
    // spelled one field gets exactly that field plus nothing.
    let flat = db::create_from_value(
        &state,
        json!({
            "name": db::fixture_name("flat-only"),
            "engine_type": "custom",
            "persistent_rooms": true,
        }),
    )
    .await;
    assert_eq!(flat.config["persistent_rooms"], json!(true));
    assert_eq!(flat.backend_mode, "wasm", "a custom game is authoritative");

    // Nested only, unchanged behaviour: the document is stored as the caller wrote it.
    let nested = db::create_from_value(
        &state,
        json!({
            "name": db::fixture_name("nested-only"),
            "config": {"tick_rate": 15, "studio_owned": "kept"},
        }),
    )
    .await;
    assert_eq!(nested.config["tick_rate"], json!(15));
    assert_eq!(nested.config["studio_owned"], json!("kept"));

    for game in [&both, &flat, &nested] {
        db::forget(&pool, game.id).await;
    }
}

/// The room-tuning document round-trips: what an operator persists is what the next
/// read reports, with the plan's own types — a boolean that arrived as a string is a
/// config the runtime cannot read.
#[tokio::test]
async fn the_persisted_config_round_trips_the_room_tuning_fields() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };

    let game = db::create_from_value(
        &state,
        json!({
            "name": db::fixture_name("config-round-trip"),
            "engine_type": "custom",
            "config": {"tick_rate": 30, "max_clients": 4, "persistent_rooms": true,
                       "auto_close_seconds": 300},
        }),
    )
    .await;
    assert_eq!(game.config["tick_rate"], json!(30));
    assert_eq!(game.config["max_clients"], json!(4));
    assert_eq!(game.config["persistent_rooms"], json!(true));
    assert_eq!(game.config["auto_close_seconds"], json!(300));

    // The typed view of the stored document agrees, which is what a plane reading the
    // column through `GameConfig` would see.
    let stored: GameConfig =
        serde_json::from_value(game.config.clone()).expect("the stored config is a GameConfig");
    assert_eq!(stored, GameConfig {
        tick_rate: 30,
        max_clients: 4,
        persistent_rooms: true,
        auto_close_seconds: 300,
    });

    // A patch replaces the document, and an absent one keeps it.
    let patched: UpdateGameRequest = serde_json::from_value(json!({"config": {
        "tick_rate": 120, "max_clients": 64, "persistent_rooms": false,
        "auto_close_seconds": 5,
    }}))
    .expect("a config patch");
    let patched = GamesService::update_game(&state, game.id, &patched)
        .await
        .expect("the second config is stored");
    assert_eq!(patched.config["tick_rate"], json!(120));
    assert_eq!(patched.config["persistent_rooms"], json!(false));
    assert_eq!(patched.config["auto_close_seconds"], json!(5));

    let renamed: UpdateGameRequest =
        serde_json::from_value(json!({"name": "Still The Same Tuning"})).expect("a patch");
    let kept = GamesService::update_game(&state, game.id, &renamed)
        .await
        .expect("a rename is applied");
    assert_eq!(kept.config, patched.config, "an absent config keeps what was stored");

    db::forget(&pool, game.id).await;
}

/// A tenant key is unique because the database says so — the console's retry loop for a
/// clashed key leans on exactly this violation, so it is asserted at the constraint.
#[tokio::test]
async fn a_game_key_is_unique_and_a_name_is_not() {
    let Some((_state, pool)) = db::state().await else {
        return skipped_without_database();
    };

    let key = format!("{GAME_KEY_PREFIX}unique_probe_{}", &Uuid::new_v4().simple().to_string()[..8]);
    // The two credentials the console mints and the mode it derives are what a hand-
    // written tenant row has to spell out: `client_key` is NOT NULL with no default, so
    // a row that skips it is refused by the schema rather than read as an empty key.
    let insert = "INSERT INTO games (game_key, name, api_key, client_key) VALUES ($1, $2, $3, $4)";
    sqlx::query(insert)
        .bind(&key)
        .bind(format!("{TEST_PREFIX} duplicate-a"))
        .bind(format!("{API_KEY_PREFIX}a"))
        .bind(format!("{CLIENT_KEY_PREFIX}a"))
        .execute(&pool)
        .await
        .expect("the first row with a key is written");

    let clash = sqlx::query(insert)
        .bind(&key)
        .bind(format!("{TEST_PREFIX} duplicate-b"))
        .bind(format!("{API_KEY_PREFIX}b"))
        .bind(format!("{CLIENT_KEY_PREFIX}b"))
        .execute(&pool)
        .await;
    let err = clash.expect_err("a duplicated game_key is refused by the UNIQUE constraint");
    assert!(
        crate::error::V2Error::is_any_unique_violation(&err),
        "and it is refused as a unique violation, not some other database error: {err}"
    );

    // Two games may share a name: it is the key, not the label, that identifies a
    // tenant, and a studio registers the same game twice on purpose.
    for label in ["same-name-a", "same-name-b"] {
        sqlx::query(insert)
            .bind(format!("{GAME_KEY_PREFIX}name_probe_{}", Uuid::new_v4().simple()))
            .bind(format!("{TEST_PREFIX} shared name"))
            .bind(format!("{API_KEY_PREFIX}{label}"))
            .bind(format!("{CLIENT_KEY_PREFIX}{label}"))
            .execute(&pool)
            .await
            .unwrap_or_else(|err| panic!("a repeated name is a fine tenant: {err}"));
    }

    // A row cannot be written without its publishable key: `client_key` is the
    // credential a built client ships, and a tenant with none is a client that cannot
    // authenticate at all.
    let missing = sqlx::query("INSERT INTO games (game_key, name, api_key) VALUES ($1, $2, $3)")
        .bind(format!("{GAME_KEY_PREFIX}no_client_key_000000"))
        .bind(format!("{TEST_PREFIX} no client key"))
        .bind(format!("{API_KEY_PREFIX}noclient"))
        .execute(&pool)
        .await;
    assert!(
        missing.is_err(),
        "`games.client_key` must stay NOT NULL, so a hand-written row without it is refused"
    );

    sqlx::query("DELETE FROM games WHERE game_key = $1 OR name = $2 OR game_key = $3")
        .bind(&key)
        .bind(format!("{TEST_PREFIX} shared name"))
        .bind(format!("{GAME_KEY_PREFIX}no_client_key_000000"))
        .execute(&pool)
        .await
        .expect("the fixtures are removed again");
}

/// The listing is the shape a dashboard renders: the publishable client key and the
/// derived mode on every row, and never the secret. Only the detail view reveals the
/// secret, together with the module aggregates.
#[tokio::test]
async fn the_listing_hides_the_secret_and_the_detail_reveals_it_with_counts() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };

    let game = db::create(&state, "listing").await;
    let detail = GamesService::get_game(&state, game.id)
        .await
        .expect("the detail view reads the game back");

    let list = GamesService::list_games(&state)
        .await
        .expect("the listing reads");
    assert!(
        list.total >= 1,
        "the listing reports the size it is showing: {}",
        list.total
    );
    let row = list
        .games
        .iter()
        .find(|row| row.id == game.id)
        .expect("a freshly registered game is in the listing");
    assert_eq!(row.game_key, game.game_key);

    // The listing is publishable-only, and that now has a positive half as well as a
    // negative one: the client key the built game ships belongs on a table a support
    // operator browses, the secret does not.
    assert_eq!(row.client_key, game.client_key);
    assert_eq!(row.backend_mode, game.backend_mode, "and so does its mode");

    // The wire shape is the contract: `games` plus `total`, and no secret anywhere in it.
    let wire: Value = serde_json::to_value(&list).expect("the listing serializes");
    assert!(wire["games"].is_array() && wire["total"].is_number(), "{wire}");
    let text = wire.to_string();
    assert!(
        !text.contains("api_key") && !text.contains(API_KEY_PREFIX),
        "a listing must never carry a tenant secret: {text}"
    );
    assert!(
        text.contains(CLIENT_KEY_PREFIX),
        "…while the publishable client key it does carry is the one a build reads: {text}"
    );

    // The detail view is the one place the secret is handed out, and it is where the
    // module counts arrive too.
    let detail_wire = serde_json::to_value(&detail).expect("the detail serializes");
    assert_eq!(detail_wire["api_key"], json!(detail.api_key));
    assert_eq!(detail_wire["client_key"], json!(detail.client_key));
    assert_eq!(detail_wire["backend_mode"], json!(detail.backend_mode));
    assert_eq!(
        detail_wire["active_module_version"],
        json!(null),
        "a game with no uploads has no version to boot at"
    );
    assert_eq!(detail_wire["module_count"], json!(0));
    assert!(
        detail_wire["config"].is_object(),
        "the detail carries the config the create wrote"
    );

    // And a game nobody registered is a refusal, not an empty row.
    let missing = GamesService::get_game(&state, Uuid::new_v4()).await;
    let err = missing.expect_err("an unknown game id is not readable");
    assert_eq!(err.kind, NOT_FOUND, "{err:?}");

    db::forget(&pool, game.id).await;
}

/// A patch changes the field it names and leaves the rest alone — including
/// `game_key`, which no patch can name at all.
#[tokio::test]
async fn update_game_changes_only_the_fields_a_patch_names() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };

    let body = CreateGameRequest {
        name: db::fixture_name("patch"),
        engine_type: Some("web".to_owned()),
        config: Some(json!({"tick_rate": 20})),
        ..Default::default()
    };
    let game = GamesService::create_game(&state, &body)
        .await
        .expect("a game with an explicit engine and config registers");

    let renamed: UpdateGameRequest = serde_json::from_value(json!({"name": "Renamed Arena"}))
        .expect("a name-only patch deserializes");
    let patched = GamesService::update_game(&state, game.id, &renamed)
        .await
        .expect("a patch that names one field is applied");

    assert_eq!(patched.name, "Renamed Arena");
    assert_eq!(patched.game_key, game.game_key, "the tenant key is not editable");
    assert_eq!(
        patched.engine_type, "web",
        "an absent engine keeps what was stored"
    );
    assert_eq!(
        patched.config["tick_rate"],
        json!(20),
        "an absent config keeps what was stored, not the defaults"
    );
    assert!(patched.updated_at >= game.updated_at);

    // A lifecycle change and a config change go together, and a suspended game is
    // still readable.
    let patched: UpdateGameRequest =
        serde_json::from_value(json!({"status": "suspended", "config": {"tick_rate": 64}}))
            .expect("a status-plus-config patch deserializes");
    let patched = GamesService::update_game(&state, game.id, &patched)
        .await
        .expect("the second patch is applied");
    assert_eq!(patched.status, "suspended");
    assert_eq!(patched.config["tick_rate"], json!(64));

    // An unknown id is the same refusal the detail view gives.
    let err = GamesService::update_game(&state, Uuid::new_v4(), &renamed)
        .await
        .expect_err("a patch to no game is not a write");
    assert_eq!(err.kind, NOT_FOUND, "{err:?}");

    db::forget(&pool, game.id).await;
}

/// A delete is a retirement: the row survives with a status that says it is gone, so
/// the modules and the history of a game nobody plays stay readable.
#[tokio::test]
async fn archive_game_is_a_soft_delete_that_keeps_the_row_and_its_key() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };

    let game = db::create(&state, "archive").await;
    let archived = GamesService::archive_game(&state, game.id)
        .await
        .expect("retiring a game is a service call, not a delete statement");
    assert!(archived, "and it reports that it archived something");

    let row = GamesService::find_game(&state, game.id)
        .await
        .expect("the row is still there, which is the whole point of a soft delete");
    assert_eq!(row.status, "archived");
    assert_eq!(row.game_key, game.game_key, "a retired tenant keeps its identity");

    // It is still listed — an operator retiring a game expects to find it — and its
    // secret is still readable, because the credential is what the history is keyed by.
    let list = GamesService::list_games(&state).await.expect("the listing reads");
    assert!(
        list.games.iter().any(|row| row.id == game.id),
        "an archived game stays in the listing"
    );
    let detail = GamesService::get_game(&state, game.id)
        .await
        .expect("the archived game is still readable in full");
    assert!(detail.api_key.starts_with(API_KEY_PREFIX));

    // A second archive is a no-op that still reads as archived, and an unknown id is
    // a refusal rather than a quiet zero.
    assert!(GamesService::archive_game(&state, game.id).await.expect("idempotent"));
    let err = GamesService::archive_game(&state, Uuid::new_v4())
        .await
        .expect_err("retiring no game is not a success");
    assert_eq!(err.kind, NOT_FOUND, "{err:?}");

    db::forget(&pool, game.id).await;
}

/// The secret is checked against the stored one, and the two failures a probe could
/// tell apart — unknown key, wrong secret — answer identically.
#[tokio::test]
async fn verify_api_key_authenticates_the_pair_and_refuses_probing() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };

    let game = db::create(&state, "secret").await;
    let detail = GamesService::get_game(&state, game.id)
        .await
        .expect("the secret is read from the detail view");

    assert!(
        GamesService::verify_api_key(&state, &detail.game_key, &detail.api_key)
            .await
            .expect("the pair the console just printed authenticates")
    );
    let wrong_secret = GamesService::verify_api_key(&state, &detail.game_key, "cyg_sec_wrong")
        .await
        .expect("a wrong secret is an authentication answer, not an error");
    assert!(!wrong_secret);
    let unknown_key = GamesService::verify_api_key(&state, "gm_no_such_game_000000", &detail.api_key)
        .await
        .expect("an unknown tenant key is the same kind of answer");
    assert!(
        !unknown_key,
        "an unknown key and a wrong secret must not be distinguishable"
    );

    // A game key on its own resolves the tenant, and its secret does not travel with
    // the lookup the module plane uses.
    let row = GamesService::find_game_by_key(&state, &detail.game_key)
        .await
        .expect("a lookup by tenant key reads")
        .expect("and finds the game that key was minted for");
    assert_eq!(row.id, game.id);
    assert!(
        GamesService::find_game_by_key(&state, "gm_never_minted_000000")
            .await
            .expect("an unknown key is a `None`, not an error")
            .is_none()
    );

    db::forget(&pool, game.id).await;
}

/// The module aggregates a detail view reports are the `room_modules` rows, read with
/// the same statements the service runs: version monotonicity, one active version, and
/// the cascade that takes a game's history with it.
#[tokio::test]
async fn the_module_aggregates_read_the_room_modules_rows() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };

    let game = db::create(&state, "modules").await;
    let sha = hex::encode([7u8; 32]);
    let insert = "INSERT INTO room_modules (game_id, version, filename, binary_bytes, source_code, sha256, is_active)
                  VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id";

    // v1, uploaded and active.
    sqlx::query(insert)
        .bind(game.id)
        .bind(1i32)
        .bind("room-v1.wasm")
        .bind(wasm_bytes().as_slice())
        .bind(None::<String>)
        .bind(&sha)
        .bind(true)
        .execute(&pool)
        .await
        .expect("the first version of a module is stored");

    assert_eq!(
        GamesService::module_count(&state, game.id)
            .await
            .expect("the count reads"),
        1,
        "one upload is one module"
    );
    assert_eq!(
        GamesService::active_module_version(&state, game.id)
            .await
            .expect("the active version reads"),
        Some(1),
        "and the newest active one is what a room boots at"
    );

    // The same version for the same game cannot be stored twice: monotonicity is a
    // constraint, not a convention the uploader has to remember.
    let clash = sqlx::query(insert)
        .bind(game.id)
        .bind(1i32)
        .bind("room-v1-again.wasm")
        .bind(wasm_bytes().as_slice())
        .bind("source kept")
        .bind(&sha)
        .bind(false)
        .execute(&pool)
        .await;
    let err = clash.expect_err("(game_id, version) is unique");
    assert!(crate::error::V2Error::is_any_unique_violation(&err), "{err}");

    // v2 arrives and takes the active flag: the uploader's own deactivation, expressed
    // as the two statements it has to be, and then read back through the service.
    let mut tx = pool.begin().await.expect("a transaction");
    sqlx::query("UPDATE room_modules SET is_active = false WHERE game_id = $1")
        .bind(game.id)
        .execute(&mut *tx)
        .await
        .expect("the older version is deactivated");
    sqlx::query(insert)
        .bind(game.id)
        .bind(2i32)
        .bind("room-v2.wasm")
        .bind(wasm_bytes().as_slice())
        .bind(Some("source kept"))
        .bind(&sha)
        .bind(true)
        .execute(&mut *tx)
        .await
        .expect("the newer version is stored active");
    tx.commit().await.expect("both statements land together");

    assert_eq!(
        GamesService::module_count(&state, game.id).await.expect("count"),
        2,
        "a newer version adds to the history rather than replacing it"
    );
    assert_eq!(
        GamesService::active_module_version(&state, game.id)
            .await
            .expect("version"),
        Some(2),
        "the newest active version wins"
    );

    // With nothing flagged active, there is no version to boot at — `None`, not zero.
    sqlx::query("UPDATE room_modules SET is_active = false WHERE game_id = $1")
        .bind(game.id)
        .execute(&pool)
        .await
        .expect("deactivating every version is representable");
    assert_eq!(
        GamesService::active_module_version(&state, game.id)
            .await
            .expect("version"),
        None,
        "a game with modules but no active one boots nothing"
    );

    // The binary and its hash are stored as written, and the source is nullable:
    // `source_code` is kept only when the uploader sent one.
    let row = sqlx::query(
        "SELECT binary_bytes, source_code, sha256 FROM room_modules WHERE game_id = $1 AND version = $2",
    )
    .bind(game.id)
    .bind(1i32)
    .fetch_one(&pool)
    .await
    .expect("v1 is readable");
    assert_eq!(
        row.get::<Vec<u8>, _>("binary_bytes"),
        wasm_bytes(),
        "the bytes the uploader sent are the bytes a room boots"
    );
    assert_eq!(row.get::<String, _>("sha256"), sha);
    assert!(
        row.get::<Option<String>, _>("source_code").is_none(),
        "an upload that sent no source stores NULL, not an empty string"
    );
    let v2 = sqlx::query("SELECT source_code FROM room_modules WHERE game_id = $1 AND version = $2")
        .bind(game.id)
        .bind(2i32)
        .fetch_one(&pool)
        .await
        .expect("v2 is readable");
    assert_eq!(
        v2.get::<Option<String>, _>("source_code").as_deref(),
        Some("source kept"),
        "an uploader that sent source has it stored verbatim"
    );

    // Deleting the tenant takes its modules with it, which is why archiving a game
    // keeps a readable history and deleting one does not orphan a binary.
    let id = game.id;
    sqlx::query("DELETE FROM games WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .expect("the game row is deletable");
    let left = sqlx::query("SELECT COUNT(*) AS n FROM room_modules WHERE game_id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("the module count reads");
    assert_eq!(left.get::<i64, _>("n"), 0, "room_modules cascades from games");
}

/// The detail view's counts and the service's own reads agree with each other — the
/// aggregate a console table prints is the same number the module plane resolves.
#[tokio::test]
async fn the_detail_view_counts_are_the_service_reads() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };

    let game = db::create(&state, "agreement").await;
    sqlx::query("INSERT INTO room_modules (game_id, version, filename, binary_bytes, sha256, is_active)
                 VALUES ($1, $2, $3, $4, $5, true)")
    .bind(game.id)
    .bind(3i32)
    .bind("room.wasm")
    .bind(wasm_bytes().as_slice())
    .bind(hex::encode([9u8; 32]))
    .execute(&pool)
    .await
    .expect("an active module at version 3");

    let detail = GamesService::get_game(&state, game.id)
        .await
        .expect("the detail view reads");
    assert_eq!(detail.module_count, GamesService::module_count(&state, game.id).await.expect("count"));
    assert_eq!(
        detail.active_module_version,
        Some(3),
        "the version the detail view prints is the version a room would boot"
    );
    assert_eq!(detail.module_count, 1);

    db::forget(&pool, game.id).await;
}

/// The plane's answer for a path, as the bytes it actually sent. The blocking harness in
/// `server.rs` parses every body as JSON, and the asset answer deliberately is not JSON,
/// so a settings download is driven through the router directly, over a state built on
/// the fixture's own pool, and read raw.
async fn fetch_raw(
    pool: &sqlx::PgPool,
    method: &str,
    path: &str,
    token: &str,
) -> (u16, Vec<(String, String)>, String) {
    use tower::ServiceExt;

    let mut state = v2_state();
    state.q = Some(std::sync::Arc::new(crate::repositories::db::Queries::new(
        pool.clone(),
    )));
    state.db = Some(pool.clone());

    let app = crate::auto_router(state).await;
    let request = axum::http::Request::builder()
        .method(method)
        .uri(path)
        .header(axum::http::header::HOST, "console.test")
        .header(axum::http::header::AUTHORIZATION, token)
        .body(axum::body::Body::empty())
        .expect("a hand-built request");
    let reply = app.oneshot(request).await.expect("the router answers");
    let status = reply.status().as_u16();
    let headers = reply
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_lowercase(),
                value.to_str().unwrap_or_default().to_owned(),
            )
        })
        .collect();
    let bytes = http_body_util::BodyExt::collect(reply.into_body())
        .await
        .expect("a body")
        .to_bytes();
    (status, headers, String::from_utf8_lossy(&bytes).into_owned())
}

/// One `key: value` line of an emitted YAML document, by name.
fn yaml_field(document: &str, key: &str) -> String {
    document
        .lines()
        .find_map(|line| {
            let (name, value) = line.trim_start().split_once(':')?;
            (name.trim() == key).then(|| value.trim().to_owned())
        })
        .unwrap_or_else(|| panic!("`{key}` is not in the asset:\n{document}"))
}

/// The asset download as the built client receives it: an attachment named
/// `CYGServerSettings.asset`, a body that is the ScriptableObject document of the
/// emitted template, and the serialized fields carrying *this* game's room defaults —
/// plus no credential anywhere in it, because the SDK class has no field to hold one.
#[tokio::test]
async fn the_unity_asset_route_answers_with_the_attached_scriptable_object() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };
    let auth = admin_token();
    let game = db::create_engine(&state, "unity-asset", Some("unity")).await;

    // The default is the asset, so a client that spells nothing gets the file.
    let (status, headers, body) = fetch_raw(
        &pool,
        "GET",
        &format!("/v2/console/games/{}/unity-settings", game.id),
        &auth,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        headers
            .iter()
            .find(|(name, _)| name == "content-disposition")
            .map(|(_, value)| value.as_str()),
        Some("attachment; filename=\"CYGServerSettings.asset\""),
        "the asset has to arrive as a file with the name the SDK expects: {headers:?}"
    );

    // The document's header, and the class it binds to.
    assert!(body.starts_with("%YAML 1.1"), "{body}");
    for marker in [
        "%TAG !u! tag:unity3d.com,2011:",
        "--- !u!114 &11400000",
        "MonoBehaviour:",
        "m_Script:",
        "m_Name: CYGServerSettings",
    ] {
        assert!(body.contains(marker), "an asset carries `{marker}`:\n{body}");
    }

    // The emitted fields are the SDK's fields, this time read off the wire rather than
    // off the template — and nothing else, so Unity drops no key and defaults none.
    let emitted = {
        let mut keys: Vec<String> = body
            .lines()
            .filter(|line| line.starts_with("  ") && !line.starts_with("   "))
            .filter_map(|line| line[2..].split_once(':'))
            .map(|(key, _)| key.trim().to_owned())
            .filter(|key| !key.is_empty() && !key.contains('_'))
            .collect();
        keys.sort();
        keys
    };
    let mut declared = sdk_serialized_fields();
    declared.sort();
    assert_eq!(emitted, declared, "the asset writes exactly the SDK's fields:\n{body}");

    // And the values are this game's: the room the plan defaults to, the capacity its
    // config names, and a protocol revision that is a number the SDK's `ushort` holds.
    assert_eq!(yaml_field(&body, "roomName"), "cyg_room");
    assert_eq!(yaml_field(&body, "serverWsPath"), "/ws");
    assert_eq!(yaml_field(&body, "matchmakeMethod"), "joinOrCreate");
    assert_eq!(
        yaml_field(&body, "maxPlayers"),
        game.config["max_clients"].to_string(),
        "the advertised capacity is the config's own: {body}"
    );
    let protocol: u32 = yaml_field(&body, "protocolVersion")
        .parse()
        .expect("protocolVersion is a number in the emitted asset");
    assert!(protocol > 0 && protocol <= u32::from(u16::MAX), "{protocol}");
    assert!(
        yaml_field(&body, "serverHttpUrl").starts_with("http://"),
        "an asset that points at a non-http origin cannot be opened by the SDK's own \
         uri parser: {body}"
    );

    // No credential rides on a file a developer commits to their project: the keys the
    // console mints live in the JSON companion, which the operator reads deliberately.
    for secret in [
        game.game_key.as_str(),
        game.client_key.as_str(),
        GAME_KEY_PREFIX,
        CLIENT_KEY_PREFIX,
        API_KEY_PREFIX,
    ] {
        assert!(
            !body.contains(secret),
            "the asset must not carry `{secret}`:\n{body}"
        );
    }

    db::forget(&pool, game.id).await;
}

/// The JSON companion is the same answer in the shape a script can read: camelCase, the
/// publishable key always, the secret only when the caller asked to reveal it.
#[tokio::test]
async fn the_unity_settings_json_carries_the_secret_only_when_revealed() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };
    let auth = admin_token();
    let game = db::create_engine(&state, "unity-json", Some("unity")).await;
    let path = format!("/v2/console/games/{}/unity-settings?format=json", game.id);

    let (status, _headers, body) = fetch_raw(&pool, "GET", &path, &auth).await;
    assert_eq!(status, 200, "{body}");
    let connection: Value = serde_json::from_str(&body).expect("the json format answers in json");
    assert_eq!(connection["gameKey"], json!(game.game_key));
    assert_eq!(connection["clientKey"], json!(game.client_key));
    assert_eq!(connection["backendMode"], json!("relay"));
    assert_eq!(connection["defaultRoom"], json!("cyg_room"));
    assert_eq!(connection["serverWsPath"], json!("/ws"));
    assert_eq!(
        connection["tickRate"], game.config["tick_rate"],
        "the connection reports the game's own tuning: {connection}"
    );
    assert!(
        connection.get("apiKey").is_none(),
        "a settings download is not a secret handout: {connection}"
    );

    // Revealed, it is the detail view's answer, and nothing else changes.
    let (status, _headers, revealed) = fetch_raw(
        &pool,
        "GET",
        &format!("{path}&reveal=true"),
        &auth,
    )
    .await;
    assert_eq!(status, 200, "{revealed}");
    let value: Value = serde_json::from_str(&revealed).expect("the revealed answer is json");
    let detail = GamesService::get_game(&state, game.id)
        .await
        .expect("the detail view reads");
    assert_eq!(value["apiKey"], json!(detail.api_key));
    assert_eq!(value["clientKey"], json!(game.client_key));
    assert_eq!(
        value["gameKey"], json!(game.game_key),
        "revealing changes the secret's presence and nothing else"
    );

    // The service's own view of the same document, and the clamp the SDK's `OnValidate`
    // would apply anyway: a config naming a capacity of a thousand is advertised as
    // 255, because that is the largest number the client will not silently rewrite.
    let roomy = db::create_from_value(
        &state,
        json!({"name": db::fixture_name("unity-clamp"), "engine_type": "web",
               "config": {"tick_rate": 20, "max_clients": 1000, "persistent_rooms": true,
                          "auto_close_seconds": 7}}),
    )
    .await;
    let connection = GamesService::unity_settings(&state, roomy.id, false, &axum::http::HeaderMap::new())
        .await
        .expect("an authoritative game has a connection document too");
    let wire = serde_json::to_value(&connection).expect("the connection serializes");
    assert_eq!(wire["maxPlayers"], json!(255), "{wire}");
    assert_eq!(wire["tickRate"], json!(20));
    assert_eq!(wire["persistentRooms"], json!(true));
    assert_eq!(wire["backendMode"], json!("wasm"), "and a wasm game is honest about being one");

    db::forget(&pool, game.id).await;
    db::forget(&pool, roomy.id).await;
}

/// A format the route does not speak is refused by name, before the game is looked up:
/// the plan defines exactly two answers, and a third spelling is a client bug, not a
/// default.
#[test]
fn an_unknown_unity_settings_format_is_refused() {
    let server = V2Server::start(v2_state());
    let reply = server.get(
        &format!(
            "/v2/console/games/{}/unity-settings?format=xml",
            Uuid::new_v4()
        ),
        Some(&admin_token()),
    );
    let message = reply.refusal(INVALID_ARGUMENT);
    assert!(
        message.contains("Unknown unity-settings format 'xml'"),
        "{message}"
    );
}

/// The settings of a game nobody registered are the same refusal the detail view gives,
/// and it is answered before a byte of asset is written.
#[tokio::test]
async fn the_unity_settings_of_a_missing_game_are_not_found() {
    let Some((state, _pool)) = db::state().await else {
        return skipped_without_database();
    };
    let err = GamesService::unity_settings(
        &state,
        Uuid::new_v4(),
        false,
        &axum::http::HeaderMap::new(),
    )
    .await
    .expect_err("no game, no settings");
    assert_eq!(err.kind, NOT_FOUND, "{err:?}");
}

// Note on the split between these two files: the statements above are the *schema's*
// rules — what `(game_id, version)` uniqueness, the nullable `source_code` and the
// cascade owe no matter which code writes the row. The *service's* rules (an insert
// deactivating the older version, the active version being undeletable, and the backend
// mode that decides whether the whole surface applies to a game at all) are asserted
// through `WasmService` itself in `wasm_upload.rs`, against rows of the same table.
