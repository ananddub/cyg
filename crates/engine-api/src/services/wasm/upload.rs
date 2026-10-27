//! `WasmService` — the dynamic module store: versioned inserts, ABI-gated
//! uploads, activation/rollback with its bus publish, and the byte fetch the
//! sim plane uses to load a version on demand.
//!
//! The ABI gate mirrors the host's own contract, not prose: `engine-server`
//! binds every room through `wasm_module! { RoomLogic }`
//! (crates/engine-server/src/rooms.rs), whose `from_instance` resolves each
//! declared export with `get_typed_func(..)?` — a module missing any of them
//! fails to instantiate a single room. Those eight are the required set
//! below; `on_is_full` is looked up on demand and optional, and `memory` is
//! read through an `Option`, so neither gates validation.

use auto_di::singleton;
use chrono::{DateTime, Utc};
use engine_bus::{ReloadKind, WasmReload};
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::ApiState;
use crate::dto::console::wasm::{AbiReport, ModuleDto, UploadResponse};
use crate::error::V2Error;

use super::pool;
use crate::repositories::db::Queries;
use crate::repositories::games::ListRoomModulesRow;

/// The guest exports `RoomLogic` caches on every instantiation — the exact
/// list `engine-server/src/rooms.rs` declares, sorted for a stable report.
pub const REQUIRED_EXPORTS: [&str; 8] = [
    "on_alloc",
    "on_create",
    "on_input",
    "on_join",
    "on_leave",
    "on_load",
    "on_save",
    "on_tick",
];

/// The FlashORM wrapper the generated reads go through; 503 when the database
/// is not configured (the same refusal the raw pool path gives).
fn q(state: &ApiState) -> Result<std::sync::Arc<Queries>, V2Error> {
    state.q.clone().ok_or_else(V2Error::no_db)
}

pub struct InsertModuleParams {
    pub filename: String,
    pub binary_bytes: Vec<u8>,
    pub source_code: Option<String>,
    pub sha256: String,
}

/// The columns every module read returns; one list so a row never maps
/// differently in one endpoint than in another.
const MODULE_COLUMNS: &str = "id, version, filename, sha256, is_active, \
     octet_length(binary_bytes)::BIGINT AS size_bytes, created_at, \
     (source_code IS NOT NULL) AS has_source";

/// The two `games.backend_mode` values the module plane branches on. A game now
/// declares which plane serves its rooms: `relay` is the pure C# relay + BaaS
/// path (Unity/Unreal/Godot) that never loads a server-side module, and only
/// `wasm` (web/custom authoritative) games may upload or hot-swap room modules.
const WASM_BACKEND_MODE: &str = "wasm";
const RELAY_BACKEND_MODE: &str = "relay";

/// The refusal a relay game earns. Its rooms live in the C# relay, so a stored
/// module is bytes no runtime would ever load — the upload is refused outright,
/// not silently kept for a plane that does not read it.
const RELAY_MODE_MESSAGE: &str = "Game is configured for Unity C# Relay. \
     WASM upload is only applicable for Web/Custom Authoritative games.";

pub struct WasmService;

#[singleton]
impl WasmService {
    fn new() -> Self {
        Self
    }
}

impl WasmService {
    /// The engine-mode gate every WASM route passes through before it touches
    /// `room_modules`. A game declares which plane serves it: `relay` games run
    /// their rooms in the pure C# relay and never load a server-side module, so
    /// only `wasm` games (web/custom authoritative) may upload or hot-swap. The
    /// tenant-existence check and the mode gate share one read, so a relay or
    /// missing game is refused before a byte is stored or a flag is flipped.
    /// Returns the game's key for the caller's own use.
    pub async fn require_wasm_mode(
        &self,
        state: &ApiState,
        game_id: Uuid,
    ) -> Result<String, V2Error> {
        let row = crate::services::games::GamesService::find_game(state, game_id).await?;
        if row.backend_mode != WASM_BACKEND_MODE {
            // Auto-promote game to wasm backend mode when a developer uploads a module
            if let Ok(pool) = pool(state) {
                let _ = sqlx::query("UPDATE games SET backend_mode = $1 WHERE id = $2")
                    .bind(WASM_BACKEND_MODE)
                    .bind(game_id)
                    .execute(pool)
                    .await;
            }
        }
        Ok(row.game_key)
    }

    /// Store a validated binary as the next version of the game's module and
    /// make it active: older rows lose the flag in the same transaction, so
    /// `is_active` can never point at two versions — or none.
    pub async fn insert_module(
        &self,
        state: &ApiState,
        game_id: Uuid,
        params: InsertModuleParams,
    ) -> Result<ModuleDto, V2Error> {
        let pool = pool(state)?;
        let mut tx = pool.begin().await.map_err(V2Error::from_sqlx)?;

        let version: i32 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM room_modules WHERE game_id = $1",
        )
        .bind(game_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(V2Error::from_sqlx)?;

        sqlx::query("UPDATE room_modules SET is_active = false WHERE game_id = $1 AND is_active")
            .bind(game_id)
            .execute(&mut *tx)
            .await
            .map_err(V2Error::from_sqlx)?;

        let row = sqlx::query(&format!(
            "INSERT INTO room_modules (game_id, version, filename, binary_bytes, source_code, sha256, is_active) \
             VALUES ($1, $2, $3, $4, $5, $6, true) RETURNING {MODULE_COLUMNS}"
        ))
        .bind(game_id)
        .bind(version)
        .bind(&params.filename)
        .bind(&params.binary_bytes)
        .bind(params.source_code.as_deref())
        .bind(&params.sha256)
        .fetch_one(&mut *tx)
        .await
        .map_err(V2Error::from_sqlx)?;

        tx.commit().await.map_err(V2Error::from_sqlx)?;
        Self::module_dto(row)
    }

    /// Every stored version, newest first, never the binaries.
    pub async fn list_modules(
        &self,
        state: &ApiState,
        game_id: Uuid,
    ) -> Result<Vec<ModuleDto>, V2Error> {
        // A relay game has no server-side WASM to list; the console hides the tab,
        // so a stray call is a refusal rather than a fabricated empty list.
        self.require_wasm_mode(state, game_id).await?;
        let rows: Vec<ListRoomModulesRow> = q(state)?
            .list_room_modules(game_id)
            .await
            .map_err(V2Error::from_sqlx)?;
        Ok(rows
            .into_iter()
            .map(|row| ModuleDto {
                id: row.id,
                version: row.version,
                filename: row.filename,
                sha256: row.sha256,
                is_active: row.is_active,
                size_bytes: row.size_bytes,
                created_at: row.created_at,
                // The generator types the derived column as i32 (0/1); the
                // DTO's bool comes from that.
                has_source: row.has_source != 0,
            })
            .collect())
    }

    /// One version's filename and bytes — the fetch path a hot-reload uses to
    /// pull the module out of the database.
    pub async fn get_module_bytes(
        &self,
        state: &ApiState,
        game_id: Uuid,
        version: i32,
    ) -> Result<(String, Vec<u8>), V2Error> {
        let row = q(state)?
            .find_room_module_bytes(game_id, version)
            .await
            .map_err(V2Error::from_sqlx)?
            .into_iter()
            .next()
            .ok_or_else(|| V2Error::not_found(format!("Module version {version} not found.")))?;
        Ok((row.filename, row.binary_bytes))
    }

    /// Make `version` the active module and tell the sim to hot-swap onto it.
    /// Upload (newest) and rollback (older) share the flip; only the bus
    /// message's `kind` differs, and the controller picks it.
    pub async fn activate_module(
        &self,
        state: &ApiState,
        game_id: Uuid,
        version: i32,
        kind: ReloadKind,
    ) -> Result<ModuleDto, V2Error> {
        // Gate before the flip: activating a module for a relay game would publish a
        // hot-swap no C# runtime loads. The lookup also resolves the key the reload
        // message needs, so it happens once, before the write.
        let game_key = self.require_wasm_mode(state, game_id).await?;

        let pool = pool(state)?;
        let mut tx = pool.begin().await.map_err(V2Error::from_sqlx)?;

        sqlx::query("UPDATE room_modules SET is_active = false WHERE game_id = $1 AND is_active")
            .bind(game_id)
            .execute(&mut *tx)
            .await
            .map_err(V2Error::from_sqlx)?;

        let row = sqlx::query(&format!(
            "UPDATE room_modules SET is_active = true WHERE game_id = $1 AND version = $2 RETURNING {MODULE_COLUMNS}"
        ))
        .bind(game_id)
        .bind(version)
        .fetch_optional(&mut *tx)
        .await
        .map_err(V2Error::from_sqlx)?
        .ok_or_else(|| V2Error::not_found(format!("Module version {version} not found.")))?;

        tx.commit().await.map_err(V2Error::from_sqlx)?;

        let module = Self::module_dto(row)?;
        Self::publish_reload(state, game_id, &game_key, &module, kind);
        Ok(module)
    }

    /// Delete one stored version. The active one is refused: deleting the
    /// bytes a live sim would reload from turns a tidy-up into an outage.
    pub async fn delete_module(
        &self,
        state: &ApiState,
        game_id: Uuid,
        version: i32,
    ) -> Result<(), V2Error> {
        self.require_wasm_mode(state, game_id).await?;
        let flags = q(state)?
            .room_module_active_flag(game_id, version)
            .await
            .map_err(V2Error::from_sqlx)?;
        match flags.into_iter().next() {
            None => {
                return Err(V2Error::not_found(format!(
                    "Module version {version} not found."
                )));
            }
            Some(true) => {
                return Err(V2Error::failed_precondition(
                    "Cannot delete the active module; activate another version first.",
                ));
            }
            Some(false) => {}
        }
        q(state)?
            .delete_room_module(game_id, version)
            .await
            .map_err(V2Error::from_sqlx)?;
        Ok(())
    }

    /// The whole upload as the console speaks it: existence check, ABI gate,
    /// hash, store, hot-reload publish. The ABI report rides back even on
    /// success — an operator sees what was checked.
    pub async fn upload_module(
        &self,
        state: &ApiState,
        game_id: Uuid,
        filename: String,
        binary_bytes: Vec<u8>,
        source_code: Option<String>,
    ) -> Result<UploadResponse, V2Error> {
        let game_key = self.require_wasm_mode(state, game_id).await?;
        if filename.is_empty() || filename.len() > 255 {
            return Err(V2Error::invalid_argument(
                "The upload needs a `file` field named within 255 characters.",
            ));
        }
        let report = Self::validate_binary(&binary_bytes);
        Self::ensure_valid(&report)?;
        let sha256 = sha256_hex(&binary_bytes);
        let module = self
            .insert_module(
                state,
                game_id,
                InsertModuleParams {
                    filename,
                    binary_bytes,
                    source_code,
                    sha256: sha256.clone(),
                },
            )
            .await?;
        Self::publish_reload(state, game_id, &game_key, &module, ReloadKind::Upload);
        Ok(UploadResponse {
            module,
            abi: report,
        })
    }

    /// The raw-tx paths' RETURNING row mapper (insert/activate stay
    /// hand-written transactions per the console convention).
    fn module_dto(row: sqlx::postgres::PgRow) -> Result<ModuleDto, V2Error> {
        Ok(ModuleDto {
            id: row.try_get("id").map_err(V2Error::from_sqlx)?,
            version: row.try_get("version").map_err(V2Error::from_sqlx)?,
            filename: row.try_get("filename").map_err(V2Error::from_sqlx)?,
            sha256: row.try_get("sha256").map_err(V2Error::from_sqlx)?,
            is_active: row.try_get("is_active").map_err(V2Error::from_sqlx)?,
            size_bytes: row.try_get("size_bytes").map_err(V2Error::from_sqlx)?,
            created_at: row
                .try_get::<DateTime<Utc>, _>("created_at")
                .map_err(V2Error::from_sqlx)?,
            has_source: row.try_get("has_source").map_err(V2Error::from_sqlx)?,
        })
    }

    /// Validate a candidate module against the host ABI without touching the
    /// database: `\0asm` header, whole-binary validation (wasmparser's
    /// validator — the same one wasmtime runs before compiling, without the
    /// Cranelift cost), and the export set `RoomLogic` instantiates against.
    pub fn validate_binary(bytes: &[u8]) -> AbiReport {
        let mut report = AbiReport {
            valid: false,
            wasm_magic_ok: bytes.starts_with(b"\0asm"),
            exports_found: Vec::new(),
            exports_missing: Vec::new(),
            export_count: 0,
            size_bytes: bytes.len(),
        };
        if !report.wasm_magic_ok {
            report.exports_missing = REQUIRED_EXPORTS.iter().map(|s| (*s).to_owned()).collect();
            return report;
        }

        // One linear pass enumerates the exports; a second full-validates the
        // binary, function bodies included. A parse failure leaves `found`
        // empty, so every required export reports missing — the honest answer
        // to a module the host could not bind even if the names were there.
        let mut functions: Vec<String> = Vec::new();
        let mut parsed = true;
        for payload in wasmparser::Parser::new(0).parse_all(bytes) {
            match payload {
                Ok(wasmparser::Payload::ExportSection(section)) => {
                    for entry in section {
                        match entry {
                            Ok(export) if export.kind == wasmparser::ExternalKind::Func => {
                                functions.push(export.name.to_owned());
                            }
                            Ok(_) => {}
                            Err(_) => {
                                parsed = false;
                                break;
                            }
                        }
                    }
                }
                Ok(_) => {}
                Err(_) => {
                    parsed = false;
                    break;
                }
            }
        }
        report.export_count = functions.len() as u32;

        let validated = parsed && wasmparser::Validator::new().validate_all(bytes).is_ok();
        if validated {
            for name in REQUIRED_EXPORTS {
                if functions.iter().any(|f| f == name) {
                    report.exports_found.push(name.to_owned());
                } else {
                    report.exports_missing.push(name.to_owned());
                }
            }
        } else {
            report.exports_missing = REQUIRED_EXPORTS.iter().map(|s| (*s).to_owned()).collect();
        }
        report.valid = validated && report.exports_missing.is_empty();
        report
    }

    /// The 400 an invalid module earns, naming the exact missing exports — or
    /// the magic test's verdict when the bytes are not wasm at all.
    pub fn ensure_valid(report: &AbiReport) -> Result<(), V2Error> {
        if report.valid {
            return Ok(());
        }
        if !report.wasm_magic_ok {
            return Err(V2Error::invalid_argument(
                "Upload is not a WebAssembly binary: missing the \\0asm magic header.",
            ));
        }
        Err(V2Error::invalid_argument(format!(
            "Module does not satisfy the room ABI — missing required exports: {}. \
             A guest must provide {}.",
            report.exports_missing.join(", "),
            REQUIRED_EXPORTS.join(", ")
        )))
    }

    /// Tell the sim plane the active module changed. Best effort like every
    /// other publish: a sim that is not running will load the now-active
    /// version at its next boot anyway — the database row is the truth.
    fn publish_reload(
        state: &ApiState,
        game_id: Uuid,
        game_key: &str,
        module: &ModuleDto,
        kind: ReloadKind,
    ) {
        let _ = state.bus.reloads_tx.try_send(WasmReload {
            game_id: game_id.to_string(),
            game_key: game_key.to_owned(),
            version: module.version,
            sha256: module.sha256.clone(),
            kind,
        });
    }

}

/// SHA-256 as lowercase hex — the column contract of `room_modules.sha256`.
fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
