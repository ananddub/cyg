use crate::services::deps::{blobs, q};

use super::*;

impl MapService {
    pub async fn delete_map(
        &self,
        state: &ApiState,
        game_id: Uuid,
        version: i32,
    ) -> Result<(), V2Error> {
        let flags = q(state).await?
            .game_map_active_flag(game_id, version)
            .await
            .map_err(V2Error::from)?;
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
        let deleted = q(state).await?
            .delete_game_map(game_id, version)
            .await
            .map_err(V2Error::from)?;
        for row in deleted {
            if let Some(key) = row.blob_key
                && let Err(err) = blobs(state).await?.delete(&key).await
            {
                tracing::warn!("[maps] orphaned blob {key}: {err}");
            }
        }
        Ok(())
    }
    pub async fn load_boot_maps(state: &ApiState) -> BootMaps {
        Self::try_load_boot_maps(state).await.unwrap_or_default()
    }
    async fn try_load_boot_maps(state: &ApiState) -> Result<BootMaps, V2Error> {
        let Some(game_id) = crate::services::games::tenant::game_id(state).await? else {
            return Ok(BootMaps::default());
        };
        let rows = q(state).await?
            .load_active_game_maps(game_id)
            .await
            .map_err(V2Error::from)?;

        let blobs = blobs(state).await?;
        let mut maps = BootMaps::default();
        for row in rows {
            let Some(format) = MapFormat::parse(&row.format) else {
                continue;
            };
            let Some(key) = row.blob_key else {
                tracing::warn!("[maps] version {} of {} has no stored bytes", row.version, row.name);
                continue;
            };
            let bytes = match blobs.get(&key).await {
                Ok(Some(bytes)) => bytes,
                Ok(None) => {
                    tracing::warn!("[maps] {} is missing from the blob store", key);
                    continue;
                }
                Err(err) => {
                    tracing::warn!("[maps] {key}: {err}");
                    continue;
                }
            };
            maps.insert(MapAsset {
                format,
                name: row.name,
                version: row.version,
                bytes,
            });
        }
        Ok(maps)
    }
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
    pub(super) fn publish_reload(state: &ApiState, game_key: &str, map: &MapDto, bytes: &[u8]) {
        let _ = state.bus.map_reloads_tx.try_send(MapReload {
            game_key: game_key.to_owned(),
            version: map.version,
            format: map.format.clone(),
            name: map.name.clone(),
            bytes: bytes.to_vec(),
        });
    }
}
