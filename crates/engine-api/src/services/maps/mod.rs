//! Game maps: the arena assets behind `/v2/console/games/{gameId}/maps*`.
//!
//! Owns the `game_maps` table — hand-written sqlx, as every console area does —
//! plus the format gates an upload must pass before it is stored, and the
//! [`engine_bus::MapReload`] publish that hands a fresh activation to the host
//! loop. The hosted game's active maps load once at boot
//! ([`MapService::load_boot_maps`]) and take priority over the project
//! folder's local map files.

use auto_di::singleton;
use chrono::{DateTime, Utc};
use engine_bus::MapReload;
use engine_config::{BootMaps, MapAsset, MapFormat};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::ApiState;
use crate::error::V2Error;
use crate::repositories::db::Queries;
use crate::repositories::game_maps::ListGameMapsRow;

/// The FlashORM wrapper the generated reads go through; 503 when the database
/// is not configured (the same refusal the raw pool path gives).
fn q(state: &ApiState) -> Result<std::sync::Arc<Queries>, V2Error> {
    state.q.clone().ok_or_else(V2Error::no_db)
}

/// The generated projection carries `has_source`-style quirks none — a plain
/// row-to-DTO move.
fn map_dto(row: ListGameMapsRow) -> MapDto {
    MapDto {
        id: row.id,
        version: row.version,
        name: row.name,
        format: row.format,
        sha256: row.sha256,
        is_active: row.is_active,
        size_bytes: row.size_bytes,
        created_at: row.created_at,
    }
}

pub struct MapService;

#[singleton]
impl MapService {
    fn new() -> Self {
        Self
    }
}

/// The pool `game_maps` lives on; 503 when the database is not configured.
fn pool(state: &ApiState) -> Result<&sqlx::PgPool, V2Error> {
    state.db.as_ref().ok_or_else(V2Error::no_db)
}

/// A cooked 3D mesh can be a few MB and a navmesh a few hundred KB; 32 MiB
/// covers both with room while still refusing the "upload my whole scene" path.
const MAX_MAP_BYTES: usize = 32 * 1024 * 1024;

/// One static-arena JSON may enumerate; beyond this the boot parse would cost
/// more than the arena is worth.
const MAX_JSON_OBSTACLES: usize = 50_000;

/// The 4-byte header a cooked OXCL mesh starts with
/// (`engine_physics::oxcl::CookedMesh`). Restated here rather than adding a
/// physics dependency to the API — the host re-parses the whole mesh at boot.
const OXCL_MAGIC: &[u8; 4] = b"OXCL";

/// The columns every map read returns; one list so a row never maps differently
/// in one endpoint than in another.
const MAP_COLUMNS: &str =
    "id, version, name, format, sha256, is_active, octet_length(bytes)::BIGINT AS size_bytes, created_at";

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MapDto {
    pub id: Uuid,
    pub version: i32,
    pub name: String,
    pub format: String,
    pub sha256: String,
    pub is_active: bool,
    pub size_bytes: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct MapUploadResponse {
    pub map: MapDto,
}

impl MapService {
    /// Every stored version of one game's maps, newest first, never the bytes.
    pub async fn list_maps(&self, state: &ApiState, game_id: Uuid) -> Result<Vec<MapDto>, V2Error> {
        let rows: Vec<ListGameMapsRow> = q(state)?
            .list_game_maps(game_id)
            .await
            .map_err(V2Error::from_sqlx)?;
        Ok(rows.into_iter().map(map_dto).collect())
    }

    /// Store a validated map as the game's next version and make it active:
    /// older rows of the same format lose the flag in the same transaction, so
    /// `is_active` can never point at two versions of a format.
    pub async fn upload_map(
        &self,
        state: &ApiState,
        game_id: Uuid,
        name: String,
        format: MapFormat,
        bytes: Vec<u8>,
    ) -> Result<MapUploadResponse, V2Error> {
        let game_key = crate::services::games::GamesService::find_game(state, game_id)
            .await?
            .game_key;
        if name.is_empty() || name.len() > 255 {
            return Err(V2Error::invalid_argument(
                "The upload needs a map name within 255 characters.",
            ));
        }
        Self::ensure_valid_format(format, &bytes)?;
        let sha256 = hex::encode(Sha256::digest(&bytes));

        let pool = pool(state)?;
        let mut tx = pool.begin().await.map_err(V2Error::from_sqlx)?;

        let version: i32 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM game_maps WHERE game_id = $1",
        )
        .bind(game_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(V2Error::from_sqlx)?;

        sqlx::query("UPDATE game_maps SET is_active = false WHERE game_id = $1 AND format = $2")
            .bind(game_id)
            .bind(format.as_str())
            .execute(&mut *tx)
            .await
            .map_err(V2Error::from_sqlx)?;

        let row = sqlx::query_as::<_, MapDto>(&format!(
            "INSERT INTO game_maps (game_id, version, name, format, bytes, sha256, is_active) \
             VALUES ($1, $2, $3, $4, $5, $6, true) RETURNING {MAP_COLUMNS}"
        ))
        .bind(game_id)
        .bind(version)
        .bind(&name)
        .bind(format.as_str())
        .bind(&bytes)
        .bind(&sha256)
        .fetch_one(&mut *tx)
        .await
        .map_err(V2Error::from_sqlx)?;
        let map = row;

        tx.commit().await.map_err(V2Error::from_sqlx)?;
        Self::publish_reload(state, &game_key, &map, &bytes);
        Ok(MapUploadResponse { map })
    }

    /// Make `version` the game's active map. Activation is exclusive per
    /// format: activating a 2D map must not retire the game's 3D mesh, so the
    /// flag flip is scoped to the version's own format.
    pub async fn activate_map(
        &self,
        state: &ApiState,
        game_id: Uuid,
        version: i32,
    ) -> Result<MapDto, V2Error> {
        let game_key = crate::services::games::GamesService::find_game(state, game_id)
            .await?
            .game_key;
        let db = pool(state)?;
        let mut tx = db.begin().await.map_err(V2Error::from_sqlx)?;

        let format: Option<String> = sqlx::query_scalar(
            "SELECT format FROM game_maps WHERE game_id = $1 AND version = $2",
        )
        .bind(game_id)
        .bind(version)
        .fetch_optional(&mut *tx)
        .await
        .map_err(V2Error::from_sqlx)?;
        let Some(format) = format else {
            return Err(V2Error::not_found(format!(
                "Map version {version} not found."
            )));
        };

        sqlx::query("UPDATE game_maps SET is_active = false WHERE game_id = $1 AND format = $2")
            .bind(game_id)
            .bind(&format)
            .execute(&mut *tx)
            .await
            .map_err(V2Error::from_sqlx)?;
        let row = sqlx::query_as::<_, MapDto>(&format!(
            "UPDATE game_maps SET is_active = true WHERE game_id = $1 AND version = $2 \
             RETURNING {MAP_COLUMNS}"
        ))
        .bind(game_id)
        .bind(version)
        .fetch_one(&mut *tx)
        .await
        .map_err(V2Error::from_sqlx)?;
        let map = row;

        tx.commit().await.map_err(V2Error::from_sqlx)?;

        // The bytes ride the bus so the host can apply the map without a
        // second database round trip.
        let bytes: Vec<u8> = sqlx::query(
            "SELECT bytes FROM game_maps WHERE game_id = $1 AND version = $2",
        )
        .bind(game_id)
        .bind(version)
        .fetch_one(pool(state)?)
        .await
        .map_err(V2Error::from_sqlx)?
        .try_get("bytes")
        .map_err(V2Error::from_sqlx)?;
        Self::publish_reload(state, &game_key, &map, &bytes);
        Ok(map)
    }

    /// Delete one stored version. The active one is refused: the bytes a boot
    /// would load must not vanish under a running game.
    pub async fn delete_map(
        &self,
        state: &ApiState,
        game_id: Uuid,
        version: i32,
    ) -> Result<(), V2Error> {
        let flags = q(state)?
            .game_map_active_flag(game_id, version)
            .await
            .map_err(V2Error::from_sqlx)?;
        match flags.into_iter().next() {
            None => {
                return Err(V2Error::not_found(format!(
                    "Map version {version} not found."
                )));
            }
            Some(true) => {
                return Err(V2Error::failed_precondition(
                    "Cannot delete the active map; activate another version first.",
                ));
            }
            Some(false) => {}
        }
        q(state)?
            .delete_game_map(game_id, version)
            .await
            .map_err(V2Error::from_sqlx)?;
        Ok(())
    }

    /// The hosted game's active maps, fetched once at boot for the host loop.
    /// No database, no resolvable game, or a failed read all yield an empty
    /// [`BootMaps`] — the local project files then apply, exactly as they did
    /// before the console existed.
    pub async fn load_boot_maps(state: &ApiState) -> BootMaps {
        Self::try_load_boot_maps(state).await.unwrap_or_default()
    }

    async fn try_load_boot_maps(state: &ApiState) -> Result<BootMaps, V2Error> {
        let Some(game_id) = crate::services::games::tenant::game_id(state).await? else {
            return Ok(BootMaps::default());
        };
        let rows = q(state)?
            .load_active_game_maps(game_id)
            .await
            .map_err(V2Error::from_sqlx)?;

        let mut maps = BootMaps::default();
        for row in rows {
            let Some(format) = MapFormat::parse(&row.format) else {
                continue;
            };
            maps.insert(MapAsset {
                format,
                name: row.name,
                version: row.version,
                bytes: row.bytes,
            });
        }
        Ok(maps)
    }

    /// The format gate, mirroring the wasm ABI gate: a map that fails here is a
    /// 400 naming the problem, never a stored row that fails at boot.
    pub fn ensure_valid_format(format: MapFormat, bytes: &[u8]) -> Result<(), V2Error> {
        if bytes.is_empty() {
            return Err(V2Error::invalid_argument("The map file is empty."));
        }
        if bytes.len() > MAX_MAP_BYTES {
            return Err(V2Error::invalid_argument(format!(
                "The map is {} MiB; the limit is 32 MiB.",
                bytes.len() / (1024 * 1024)
            )));
        }
        match format {
            MapFormat::Oxcl => {
                if !bytes.starts_with(OXCL_MAGIC) {
                    return Err(V2Error::invalid_argument(
                        "Not an OXCL mesh: cook a .glb first (`ox-game cook <map.glb> <out.oxcl>`) \
                         and upload the cooked file.",
                    ));
                }
            }
            MapFormat::Map2d => Self::validate_arena_json(bytes, &["arena_width", "arena_height"])?,
            MapFormat::Map3d => Self::validate_arena_json(bytes, &["arena_width", "arena_depth"])?,
            MapFormat::Navmesh => {
                if bytes.len() < 8 {
                    return Err(V2Error::invalid_argument(
                        "The navmesh file is too small to be a baked mesh.",
                    ));
                }
            }
        }
        Ok(())
    }

    /// The two static-arena JSON dialects share one shape check: parses as an
    /// object, carries its dimension fields as numbers, and does not smuggle a
    /// five-hundred-thousand-obstacle boot stall.
    fn validate_arena_json(bytes: &[u8], dims: &[&str]) -> Result<(), V2Error> {
        let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| {
            V2Error::invalid_argument(format!("The arena map is not valid JSON: {e}"))
        })?;
        let obj = value
            .as_object()
            .ok_or_else(|| V2Error::invalid_argument("The arena map must be a JSON object."))?;
        for dim in dims {
            if !obj.get(*dim).is_some_and(|v| v.is_number()) {
                return Err(V2Error::invalid_argument(format!(
                    "The arena map is missing the numeric `{dim}` field."
                )));
            }
        }
        if let Some(obstacles) = obj.get("obstacles") {
            let count = obstacles.as_array().map_or(0, |a| a.len());
            if count > MAX_JSON_OBSTACLES {
                return Err(V2Error::invalid_argument(format!(
                    "The arena lists {count} obstacles; the limit is {MAX_JSON_OBSTACLES}."
                )));
            }
        }
        Ok(())
    }

    /// Tell the host loop the active map changed. Best effort like every other
    /// publish: a host that is not running loads the now-active version at its
    /// next boot anyway — the database row is the truth.
    fn publish_reload(state: &ApiState, game_key: &str, map: &MapDto, bytes: &[u8]) {
        let _ = state.bus.map_reloads_tx.try_send(MapReload {
            game_key: game_key.to_owned(),
            version: map.version,
            format: map.format.clone(),
            name: map.name.clone(),
            bytes: bytes.to_vec(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2D arena that passes the gate: parses, carries its dimensions.
    fn good_2d() -> Vec<u8> {
        br#"{"arena_width": 800, "arena_height": 600, "obstacles": [{"x":0,"y":0,"w":10,"h":10}]}"#.to_vec()
    }

    #[test]
    fn map2d_gate_accepts_a_well_formed_arena() {
        assert!(MapService::ensure_valid_format(MapFormat::Map2d, &good_2d()).is_ok());
    }

    #[test]
    fn arena_json_without_its_dimension_field_is_refused() {
        let bad = br#"{"obstacles": []}"#.to_vec();
        let err = MapService::ensure_valid_format(MapFormat::Map2d, &bad).unwrap_err();
        assert!(err.message.contains("arena_width"), "{}", err.message);
    }

    #[test]
    fn map3d_gate_expects_arena_depth_not_height() {
        let depth = br#"{"arena_width": 100, "arena_depth": 200}"#.to_vec();
        let height = br#"{"arena_width": 100, "arena_height": 200}"#.to_vec();
        assert!(MapService::ensure_valid_format(MapFormat::Map3d, &depth).is_ok());
        let err = MapService::ensure_valid_format(MapFormat::Map3d, &height).unwrap_err();
        assert!(err.message.contains("arena_depth"), "{}", err.message);
    }

    #[test]
    fn non_json_arena_bytes_are_refused_with_the_parse_reason() {
        let err = MapService::ensure_valid_format(MapFormat::Map2d, b"not json").unwrap_err();
        assert!(err.message.contains("not valid JSON"), "{}", err.message);
    }

    #[test]
    fn oxcl_gate_requires_the_cooked_magic() {
        let cooked = {
            let mut bytes = b"OXCL".to_vec();
            bytes.extend_from_slice(&[0u8; 32]);
            bytes
        };
        assert!(MapService::ensure_valid_format(MapFormat::Oxcl, &cooked).is_ok());
        let err = MapService::ensure_valid_format(MapFormat::Oxcl, b"glTF raw scene bytes").unwrap_err();
        assert!(err.message.contains("OXCL"), "{}", err.message);
    }

    #[test]
    fn empty_and_oversized_uploads_are_refused() {
        assert!(MapService::ensure_valid_format(MapFormat::Navmesh, &[]).is_err());
        let oversized = vec![0u8; MAX_MAP_BYTES + 1];
        assert!(MapService::ensure_valid_format(MapFormat::Navmesh, &oversized).is_err());
        // A navmesh only needs to not be trivially tiny; deep parsing happens
        // on the host at boot.
        assert!(MapService::ensure_valid_format(MapFormat::Navmesh, &[0u8; 8]).is_ok());
    }

    #[test]
    fn boot_maps_last_active_version_of_a_format_wins() {
        let mut maps = BootMaps::default();
        assert!(maps.is_empty());
        maps.insert(MapAsset {
            format: MapFormat::Map2d,
            name: "a".into(),
            version: 1,
            bytes: good_2d(),
        });
        maps.insert(MapAsset {
            format: MapFormat::Map2d,
            name: "b".into(),
            version: 2,
            bytes: good_2d(),
        });
        assert!(!maps.is_empty());
        assert_eq!(maps.map2d.as_ref().unwrap().name, "b");
        assert!(maps.oxcl.is_none());
    }

    #[test]
    fn format_parse_is_the_str_table_the_upload_and_host_share() {
        assert_eq!(MapFormat::parse("oxcl"), Some(MapFormat::Oxcl));
        assert_eq!(MapFormat::parse("map2d"), Some(MapFormat::Map2d));
        assert_eq!(MapFormat::parse("map3d"), Some(MapFormat::Map3d));
        assert_eq!(MapFormat::parse("navmesh"), Some(MapFormat::Navmesh));
        assert_eq!(MapFormat::parse("scene.gltf"), None);
        for format in [
            MapFormat::Oxcl,
            MapFormat::Map2d,
            MapFormat::Map3d,
            MapFormat::Navmesh,
        ] {
            assert_eq!(MapFormat::parse(format.as_str()), Some(format));
        }
    }
}
