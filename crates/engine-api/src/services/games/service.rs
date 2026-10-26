//! The games a console operator registers — the tenancy root every other game-plane
//! object hangs off of.
//!
//! Hand-written SQL, like the rest of the console: `games` is a table this feature
//! owns end to end, and the reads the module plane needs (existence by id, lookup by
//! key, secret check) are one query each. What is deliberately *not* here is any way
//! for a client to name its own `game_key`, `api_key` or `client_key` — all three are
//! minted server-side, so a tenant credential is never something a caller could guess
//! or choose. Neither is `backend_mode`: it is a property of the engine, derived by
//! [`backend_mode_for_engine`], never a field a body can set.

use auto_di::singleton;
use axum::http::HeaderMap;
use rand::Rng;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::ApiState;
use crate::dto::console::games::{
    CreateGameRequest, ENGINE_TYPES, GAME_STATUSES, GameConfig, GameDetailDto, GameDto,
    GameListDto, GameRow, UnityConnectionDto, UpdateGameRequest,
};
use crate::error::V2Error;
use crate::utils::status::client_host;

use super::schema::DEFAULT_GAME_CONFIG_JSON;

pub struct GamesService;

#[singleton]
impl GamesService {
    fn new() -> Self {
        Self
    }
}

/// `POST /v2/console/games`.
const INSERT_GAME: &str = "INSERT INTO games (game_key, name, api_key, client_key, engine_type, backend_mode, status, config)
VALUES ($1, $2, $3, $4, $5, $6, 'active', $7)
RETURNING id, game_key, name, client_key, engine_type, backend_mode, status, config, created_at, updated_at;";

/// `GET /v2/console/games`, newest first: a console with forty games wants to see the
/// one it just added.
const SELECT_GAMES: &str = "SELECT id, game_key, name, client_key, engine_type, backend_mode, status, config, created_at, updated_at
FROM games
ORDER BY created_at DESC;";

const SELECT_GAME_COUNT: &str = "SELECT COUNT(*)::BIGINT AS total FROM games;";

const SELECT_GAME_BY_ID: &str = "SELECT id, game_key, name, client_key, engine_type, backend_mode, status, config, created_at, updated_at
FROM games
WHERE id = $1;";

const SELECT_GAME_BY_KEY: &str = "SELECT id, game_key, name, client_key, engine_type, backend_mode, status, config, created_at, updated_at
FROM games
WHERE game_key = $1;";

/// The secret, read on its own so it can never ride along with a listing.
const SELECT_GAME_SECRET: &str = "SELECT api_key FROM games WHERE id = $1;";

/// The secret as the module plane resolves it — by the key the caller presented.
const SELECT_SECRET_BY_KEY: &str = "SELECT api_key FROM games WHERE game_key = $1;";

/// `PUT /v2/console/games/{gameId}`: every field is optional, and an absent one keeps
/// its stored value rather than being written as a default. `backend_mode` is not a
/// patchable field: it is recomputed from the engine whenever the patch names one.
const UPDATE_GAME: &str = "UPDATE games
SET name = COALESCE($1, name),
    engine_type = COALESCE($2, engine_type),
    status = COALESCE($3, status),
    config = COALESCE($4, config),
    backend_mode = COALESCE($5, backend_mode),
    updated_at = NOW()
WHERE id = $6
RETURNING id, game_key, name, client_key, engine_type, backend_mode, status, config, created_at, updated_at;";

/// `DELETE /v2/console/games/{gameId}` — a soft delete. The row and, through it, its
/// modules survive, so a game's history is still readable after it is retired.
const ARCHIVE_GAME: &str =
    "UPDATE games SET status = 'archived', updated_at = NOW() WHERE id = $1;";

/// The version a room should boot at: the newest one flagged active.
const SELECT_ACTIVE_MODULE_VERSION: &str = "SELECT version FROM room_modules WHERE game_id = $1 AND is_active ORDER BY version DESC LIMIT 1;";

const SELECT_MODULE_COUNT: &str =
    "SELECT COUNT(*)::BIGINT AS total FROM room_modules WHERE game_id = $1;";

/// How many times a create retries when the minted key clashes. A 24-bit suffix
/// colliding is rare, and a collision is a retry rather than an operator error.
const KEY_ATTEMPTS: usize = 4;

/// The default room the Unity SDK joins when the game names none — the value of
/// `CYGServerSettings.DefaultRoomName`, restated so the asset binds to a room the
/// matchmake plane will actually create.
const DEFAULT_ROOM_NAME: &str = "cyg_room";

/// The path prefix the game socket is upgraded on: the plane's `#[auto_ws("/ws")]`
/// mount, which is what `CYGServerSettings.serverWsPath` must name.
const SERVER_WS_PATH: &str = "/ws";

impl GamesService {
    /// Register a game. The name is the only required field: the key, the two
    /// credentials, the backend mode and the config all have server-side answers.
    pub async fn create_game(
        state: &ApiState,
        body: &CreateGameRequest,
    ) -> Result<GameDto, V2Error> {
        let name = game_name(&body.name)?;
        let engine = engine_type(body.engine_type.as_deref().unwrap_or("unity"))?;
        let backend = backend_mode_for_engine(&engine);
        let config = create_config(body)?;
        let pool = pool(state)?;

        // The only clash possible here is on the minted key: the name is not unique,
        // and the id is the database's. Retrying with a fresh suffix is the whole
        // recovery — no client ever sees this message unless the draw is unlucky four
        // times running.
        for _ in 0..KEY_ATTEMPTS {
            let row = sqlx::query_as::<_, GameRow>(INSERT_GAME)
                .bind(game_key(&name))
                .bind(&name)
                .bind(api_key())
                .bind(client_key())
                .bind(&engine)
                .bind(backend)
                .bind(&config)
                .fetch_optional(pool)
                .await;
            match row {
                Ok(Some(row)) => return Ok(GameDto::from(row)),
                Ok(None) => {
                    return Err(V2Error::internal(
                        "The game was written but could not be read back.",
                    ));
                }
                Err(err) if V2Error::is_any_unique_violation(&err) => continue,
                Err(err) => return Err(V2Error::from_sqlx(err)),
            }
        }
        Err(V2Error::already_exists(
            "This game's name keeps colliding with an existing key; register it again.",
        ))
    }

    /// Every game, with its size. Archived games are listed — an operator retiring a
    /// game still expects to find it.
    pub async fn list_games(state: &ApiState) -> Result<GameListDto, V2Error> {
        let pool = pool(state)?;
        let rows = sqlx::query_as::<_, GameRow>(SELECT_GAMES)
            .fetch_all(pool)
            .await
            .map_err(V2Error::from_sqlx)?;
        let total = sqlx::query(SELECT_GAME_COUNT)
            .fetch_one(pool)
            .await
            .map(|row| row.get::<i64, _>("total"))
            .map_err(V2Error::from_sqlx)?;
        Ok(GameListDto {
            games: rows.into_iter().map(GameDto::from).collect(),
            total,
        })
    }

    /// One game in full, secret included. The secret is only ever revealed here, which
    /// is the console's equivalent of a rotate screen.
    pub async fn get_game(state: &ApiState, game_id: Uuid) -> Result<GameDetailDto, V2Error> {
        let row = Self::find_game(state, game_id).await?;
        let pool = pool(state)?;
        let api_key = sqlx::query(SELECT_GAME_SECRET)
            .bind(game_id)
            .fetch_optional(pool)
            .await
            .map_err(V2Error::from_sqlx)?
            .map(|row| row.get::<String, _>("api_key"))
            .ok_or_else(|| V2Error::not_found(NOT_FOUND))?;
        Ok(GameDetailDto {
            id: row.id,
            game_key: row.game_key,
            name: row.name,
            client_key: row.client_key,
            engine_type: row.engine_type,
            backend_mode: row.backend_mode,
            status: row.status,
            config: row.config,
            created_at: row.created_at,
            updated_at: row.updated_at,
            api_key,
            active_module_version: Self::active_module_version(state, game_id).await?,
            module_count: Self::module_count(state, game_id).await?,
        })
    }

    /// `GET /v2/console/games/{gameId}/unity-settings` — the connection companion of
    /// the generated Unity asset. `api_key` is carried only when the caller asked with
    /// `reveal`; `client_key`, being publishable, always travels. The runtime values
    /// come from the game's own config document, read through [`GameConfig`] so a
    /// partial document still answers with complete defaults; `max_players` is clamped
    /// to the 1..=255 the SDK's `OnValidate` enforces, so an asset never shows a
    /// capacity the client will silently rewrite.
    pub async fn unity_settings(
        state: &ApiState,
        game_id: Uuid,
        reveal: bool,
        headers: &HeaderMap,
    ) -> Result<UnityConnectionDto, V2Error> {
        let detail = Self::get_game(state, game_id).await?;
        let config =
            serde_json::from_value::<GameConfig>(detail.config.clone()).unwrap_or_default();
        Ok(UnityConnectionDto {
            game_key: detail.game_key,
            client_key: detail.client_key,
            api_key: reveal.then_some(detail.api_key),
            server_http_url: advertised_base_url(state, headers),
            server_ws_path: SERVER_WS_PATH.to_owned(),
            default_room: DEFAULT_ROOM_NAME.to_owned(),
            tick_rate: config.tick_rate,
            max_players: config.max_clients.clamp(1, 255),
            persistent_rooms: config.persistent_rooms,
            backend_mode: detail.backend_mode,
        })
    }

    /// Patch a game. At least one field must be named — an empty patch would rewrite
    /// `updated_at` for no reason and read as a lost write.
    pub async fn update_game(
        state: &ApiState,
        game_id: Uuid,
        body: &UpdateGameRequest,
    ) -> Result<GameDto, V2Error> {
        let name = body.name.as_deref().map(game_name).transpose()?;
        let engine = body.engine_type.as_deref().map(engine_type).transpose()?;
        // A patch that changes the engine moves the game between the relay and the
        // WASM planes, so the mode rides along with it; every other patch leaves it.
        let backend = engine.as_deref().map(backend_mode_for_engine);
        let status = body.status.as_deref().map(game_status).transpose()?;

        // The flat config fields patch the stored document one key at a time —
        // the same "flat wins over nested" merge registration uses — so a
        // sync-rate change never needs the caller to resend the whole config. A
        // nested `config` object alone still replaces the document outright.
        let config = if body.tick_rate.is_none()
            && body.max_clients.is_none()
            && body.persistent_rooms.is_none()
            && body.auto_close_seconds.is_none()
        {
            config_value(body.config.as_ref())?
        } else {
            let current = Self::find_game(state, game_id).await?.config;
            let mut map = current.as_object().cloned().unwrap_or_default();
            if let Some(patch) = config_value(body.config.as_ref())? {
                if let Some(patch_map) = patch.as_object() {
                    for (key, value) in patch_map {
                        map.insert(key.clone(), value.clone());
                    }
                }
            }
            if let Some(v) = body.tick_rate {
                map.insert("tick_rate".to_owned(), json!(v));
            }
            if let Some(v) = body.max_clients {
                map.insert("max_clients".to_owned(), json!(v));
            }
            if let Some(v) = body.persistent_rooms {
                map.insert("persistent_rooms".to_owned(), json!(v));
            }
            if let Some(v) = body.auto_close_seconds {
                map.insert("auto_close_seconds".to_owned(), json!(v));
            }
            Some(serde_json::Value::Object(map))
        };

        if name.is_none() && engine.is_none() && status.is_none() && config.is_none() {
            return Err(V2Error::invalid_argument(
                "A game update has to change at least one field.",
            ));
        }

        let row = sqlx::query_as::<_, GameRow>(UPDATE_GAME)
            .bind(name)
            .bind(engine)
            .bind(status)
            .bind(config)
            .bind(backend)
            .bind(game_id)
            .fetch_optional(pool(state)?)
            .await
            .map_err(V2Error::from_sqlx)?
            .ok_or_else(|| V2Error::not_found(NOT_FOUND))?;
        Ok(GameDto::from(row))
    }

    /// Retire a game. Its modules, its key and its secret stay in the database, so the
    /// history of a game nobody plays is still readable; what changes is the status a
    /// caller checks — [`Self::find_game`] hands it out, and the module plane refuses a
    /// game that is not `active`.
    pub async fn archive_game(state: &ApiState, game_id: Uuid) -> Result<bool, V2Error> {
        // Read the row first so an unknown id is a 404 rather than a quiet no-op.
        Self::find_game(state, game_id).await?;
        let done = sqlx::query(ARCHIVE_GAME)
            .bind(game_id)
            .execute(pool(state)?)
            .await
            .map_err(V2Error::from_sqlx)?;
        Ok(done.rows_affected() > 0)
    }

    /// A game by id, secret excluded. This is the existence check the module plane
    /// runs before it stores an upload: it needs the tenant's name and status, and has
    /// no business seeing its credential.
    pub async fn find_game(state: &ApiState, game_id: Uuid) -> Result<GameRow, V2Error> {
        sqlx::query_as::<_, GameRow>(SELECT_GAME_BY_ID)
            .bind(game_id)
            .fetch_optional(pool(state)?)
            .await
            .map_err(V2Error::from_sqlx)?
            .ok_or_else(|| V2Error::not_found(NOT_FOUND))
    }

    /// A game by its tenant key, if it has one. `None` is not an error: an unknown key
    /// is an authentication failure, not a missing row a client could go look up.
    pub async fn find_game_by_key(
        state: &ApiState,
        game_key: &str,
    ) -> Result<Option<GameRow>, V2Error> {
        sqlx::query_as::<_, GameRow>(SELECT_GAME_BY_KEY)
            .bind(game_key)
            .fetch_optional(pool(state)?)
            .await
            .map_err(V2Error::from_sqlx)
    }

    /// Check a presented secret against the stored one. An unknown key and a wrong
    /// secret answer identically — `false` — so the route cannot be walked by probing
    /// which half of the pair was wrong.
    ///
    /// The secret only: a suspended or archived game still verifies, because whether a
    /// live tenant is allowed to act is a lifecycle question, not a credential one. A
    /// caller that has to refuse an archived game asks [`Self::find_game_by_key`].
    pub async fn verify_api_key(
        state: &ApiState,
        game_key: &str,
        api_key: &str,
    ) -> Result<bool, V2Error> {
        let Some(stored) = sqlx::query(SELECT_SECRET_BY_KEY)
            .bind(game_key)
            .fetch_optional(pool(state)?)
            .await
            .map_err(V2Error::from_sqlx)?
        else {
            return Ok(false);
        };
        Ok(secrets_match(
            stored.get::<String, _>("api_key").as_bytes(),
            api_key.as_bytes(),
        ))
    }

    /// The version a room of this game should boot at, or `None` when the game has no
    /// active module yet.
    pub async fn active_module_version(
        state: &ApiState,
        game_id: Uuid,
    ) -> Result<Option<i32>, V2Error> {
        Ok(sqlx::query(SELECT_ACTIVE_MODULE_VERSION)
            .bind(game_id)
            .fetch_optional(pool(state)?)
            .await
            .map_err(V2Error::from_sqlx)?
            .map(|row| row.get::<i32, _>("version")))
    }

    /// How many modules a game has uploaded, active or not.
    pub async fn module_count(state: &ApiState, game_id: Uuid) -> Result<i64, V2Error> {
        Ok(sqlx::query(SELECT_MODULE_COUNT)
            .bind(game_id)
            .fetch_one(pool(state)?)
            .await
            .map(|row| row.get::<i64, _>("total"))
            .map_err(V2Error::from_sqlx)?)
    }
}

/// The console is entirely database-backed; without a pool every game view is a 503.
fn pool(state: &ApiState) -> Result<&sqlx::PgPool, V2Error> {
    state.db.as_ref().ok_or_else(V2Error::no_db)
}

/// What a missing game is called, in the one wording the console uses everywhere.
const NOT_FOUND: &str = "Game not found.";

/// A game needs a name an operator can recognise in a list, and the column is
/// `VARCHAR(128)`, so a longer one is refused rather than truncated by the database.
fn game_name(raw: &str) -> Result<String, V2Error> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(V2Error::invalid_argument("A game needs a name."));
    }
    if name.chars().count() > 128 {
        return Err(V2Error::invalid_argument(
            "A game name is at most 128 characters.",
        ));
    }
    Ok(name.to_owned())
}

/// One of the five engines the plane knows how to run. Anything else is refused rather
/// than stored and discovered at the first room boot.
fn engine_type(raw: &str) -> Result<String, V2Error> {
    let engine = raw.trim().to_ascii_lowercase();
    if ENGINE_TYPES.contains(&engine.as_str()) {
        Ok(engine)
    } else {
        Err(V2Error::invalid_argument(format!(
            "Unknown engine type '{raw}'; expected one of {}.",
            ENGINE_TYPES.join(", ")
        )))
    }
}

/// Which backend an engine plays on: `unity`, `unreal` and `godot` are served by the
/// pure C# relay plus the BaaS plane (`"relay"`, and the WASM upload plane does not
/// apply to them); `web` and `custom` run the authoritative server simulation, so
/// `"wasm"`. The single derivation is what makes the column unforgeable: no request
/// body names a mode, and [`GamesService::update_game`] recomputes it whenever the
/// engine moves.
pub fn backend_mode_for_engine(engine_type: &str) -> &'static str {
    match engine_type {
        "web" | "custom" => "wasm",
        // `unity` is the schema's default engine, and the whitelist admits nothing
        // else besides the two above, so anything reaching here relays.
        _ => "relay",
    }
}

/// One of the three lifecycles.
fn game_status(raw: &str) -> Result<String, V2Error> {
    let status = raw.trim().to_ascii_lowercase();
    if GAME_STATUSES.contains(&status.as_str()) {
        Ok(status)
    } else {
        Err(V2Error::invalid_argument(format!(
            "Unknown game status '{raw}'; expected one of {}.",
            GAME_STATUSES.join(", ")
        )))
    }
}

/// The configuration document. It must be an object — a bare number or a list would
/// deserialize into nothing the runtime reads — but unknown keys inside it are kept, so
/// an engine can carry its own settings without a schema change here.
fn config_value(raw: Option<&serde_json::Value>) -> Result<Option<serde_json::Value>, V2Error> {
    match raw {
        None => Ok(None),
        Some(value @ serde_json::Value::Object(_)) => Ok(Some(value.clone())),
        Some(_) => Err(V2Error::invalid_argument(
            "A game config must be a JSON object.",
        )),
    }
}

/// The tenant key: `gm_<slug>_<6 hex>`. The slug is the name, lowercased and reduced to
/// the characters a key may hold, because an operator searches for a game by the name
/// they typed.
fn game_key(name: &str) -> String {
    let slug: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(50)
        .collect();
    let slug = slug.trim_matches('_');
    let slug = if slug.is_empty() { "game" } else { slug };
    // 24 bits, printed as six hex digits: enough to make two games of the same name
    // collide about once in 16 million, and short enough that a key stays readable.
    let suffix = rand::thread_rng().gen_range(0..0x100_0000u32);
    format!("gm_{slug}_{suffix:06x}")
}

/// The secret: a 256-bit hash of fresh randomness, hex-encoded, under the `cyg_sec_`
/// prefix the console's rotate screen and the module plane both expect. Hashing the
/// random bytes rather than hex-encoding them keeps the value uniform no matter what
/// the generator hands back.
fn api_key() -> String {
    credential(super::API_KEY_PREFIX)
}

/// The public client credential: minted exactly like the secret — same generator, same
/// width — under the `cyg_pub_` prefix, and a distinct value. It ships inside the
/// built game, so it is publishable by design; the secret stays on the server.
fn client_key() -> String {
    credential(super::CLIENT_KEY_PREFIX)
}

/// The shared mint behind [`api_key`] and [`client_key`]: 8..12 prefix characters plus
/// 64 hex, always well inside the columns' `VARCHAR(128)`. The prefix is what makes a
/// credential recognisable in a log line without being the credential.
fn credential(prefix: &str) -> String {
    let mut bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
    let digest = Sha256::digest(bytes);
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("{prefix}{hex}")
}

/// Compare two secrets without the comparison time depending on where they first
/// differ, so a wrong character costs the same as a wrong length.
fn secrets_match(stored: &[u8], presented: &[u8]) -> bool {
    let mut diff = stored.len() ^ presented.len();
    for i in 0..stored.len().min(presented.len()) {
        diff |= (stored[i] ^ presented[i]) as usize;
    }
    diff == 0
}

/// The base url a generated Unity build should dial. `engine-config` carries no
/// advertised public URL (`ControlConfig` knows only its `bind_addr` and `api_port`),
/// so the plane echoes the authority the caller itself used, re-pinned to this
/// plane's port — the same rule `/api/rooms` applies to its `serverId`. A TLS
/// terminalling proxy announces itself with `x-forwarded-proto: https`; without it
/// the honest answer is plain http.
fn advertised_base_url(state: &ApiState, headers: &HeaderMap) -> String {
    let forwarded = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let scheme = if forwarded.trim().eq_ignore_ascii_case("https") {
        "https"
    } else {
        "http"
    };
    format!(
        "{scheme}://{}:{}",
        client_host(headers),
        state.control_port()
    )
}

/// What a create writes into `config`: the document the caller named, or the column's
/// own default restated, so a game that was given no configuration still reports a
/// complete one on its first read.
fn stored_config(raw: Option<&serde_json::Value>) -> Result<serde_json::Value, V2Error> {
    match config_value(raw)? {
        Some(config) => Ok(config),
        None => serde_json::from_str(DEFAULT_GAME_CONFIG_JSON)
            .or_else(|_| serde_json::to_value(GameConfig::default()))
            .map_err(|_| V2Error::internal("The default game config is not valid JSON.")),
    }
}

/// The create's config in either spelling: the nested `config` document, or the plan's
/// flat fields (`tick_rate`, `max_clients`, `persistent_rooms`,
/// `auto_close_seconds`) beside the name. The nested document — or, without one, the
/// complete default — is the base; every flat field that is present overwrites its
/// key, so when a body names both the flat value wins. Unknown keys inside `config`
/// survive untouched: it is a JSONB document, not a checked schema.
fn create_config(body: &CreateGameRequest) -> Result<serde_json::Value, V2Error> {
    let mut config = stored_config(body.config.as_ref())?;
    let map = config
        .as_object_mut()
        .expect("stored_config only returns objects");
    if let Some(tick_rate) = body.tick_rate {
        map.insert("tick_rate".to_owned(), json!(tick_rate));
    }
    if let Some(max_clients) = body.max_clients {
        map.insert("max_clients".to_owned(), json!(max_clients));
    }
    if let Some(persistent_rooms) = body.persistent_rooms {
        map.insert("persistent_rooms".to_owned(), json!(persistent_rooms));
    }
    if let Some(auto_close_seconds) = body.auto_close_seconds {
        map.insert("auto_close_seconds".to_owned(), json!(auto_close_seconds));
    }
    Ok(config)
}
