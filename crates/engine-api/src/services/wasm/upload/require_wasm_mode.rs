use crate::services::deps::{blobs, q};

use super::*;

impl WasmService {
    pub async fn require_wasm_mode(
        &self,
        state: &ApiState,
        game_id: Uuid,
    ) -> Result<String, V2Error> {
        let row = crate::services::games::GamesService::find_game(state, game_id).await?;
        if row.backend_mode != WASM_BACKEND_MODE
            && let Ok(queries) = q(state).await
        {
            let _ = queries
                .set_game_backend_mode(game_id, WASM_BACKEND_MODE)
                .await;
        }
        Ok(row.game_key)
    }
    pub async fn insert_module(
        &self,
        state: &ApiState,
        game_id: Uuid,
        params: InsertModuleParams,
    ) -> Result<ModuleDto, V2Error> {
        let queries = q(state).await?;
        let version = queries
            .latest_room_module_version(game_id)
            .await
            .map_err(V2Error::from)?
            .unwrap_or(0)
            + 1;

        let id = Uuid::new_v4();
        let now = Utc::now();
        let key = module_key(game_id, version);
        blobs(state)
            .await?
            .put(&key, &params.binary_bytes)
            .await
            .map_err(V2Error::from)?;
        let data = json!({
            "game_id": game_id.to_string(),
            "version": version,
            "filename": params.filename.clone(),
            "blob_key": key.clone(),
            "size_bytes": params.binary_bytes.len() as i64,
            "source_code": params.source_code.clone(),
            "sha256": params.sha256.clone(),
            "is_active": true,
            "created_at": time::encode(now),
        });
        if let Err(err) = queries.insert_room_module(game_id, id, &data).await {
            let _ = blobs(state).await?.delete(&key).await;
            return Err(V2Error::from(err));
        }

        Ok(ModuleDto {
            id,
            version,
            filename: params.filename,
            sha256: params.sha256,
            is_active: true,
            size_bytes: params.binary_bytes.len() as i64,
            created_at: now,
            has_source: params.source_code.is_some(),
        })
    }
    pub async fn list_modules(
        &self,
        state: &ApiState,
        game_id: Uuid,
    ) -> Result<Vec<ModuleDto>, V2Error> {
        self.require_wasm_mode(state, game_id).await?;
        let rows: Vec<ListRoomModulesRow> = q(state)
            .await?
            .list_room_modules(game_id)
            .await
            .map_err(V2Error::from)?;
        Ok(rows.into_iter().map(module_dto).collect())
    }
    pub async fn get_module_bytes(
        &self,
        state: &ApiState,
        game_id: Uuid,
        version: i32,
    ) -> Result<(String, Vec<u8>), V2Error> {
        let row = q(state)
            .await?
            .find_room_module_bytes(game_id, version)
            .await
            .map_err(V2Error::from)?
            .into_iter()
            .next()
            .ok_or_else(|| V2Error::not_found(format!("Module version {version} not found.")))?;
        let Some(key) = row.blob_key else {
            return Err(V2Error::not_found(format!(
                "Module version {version} bytes are missing."
            )));
        };
        let bytes = blobs(state)
            .await?
            .get(&key)
            .await
            .map_err(V2Error::from)?
            .ok_or_else(|| {
                V2Error::not_found(format!("Module version {version} bytes are missing."))
            })?;
        Ok((row.filename, bytes))
    }
}
