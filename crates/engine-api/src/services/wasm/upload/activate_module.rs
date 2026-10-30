use crate::services::deps::{blobs, q};

use super::*;

impl WasmService {
    pub async fn activate_module(
        &self,
        state: &ApiState,
        game_id: Uuid,
        version: i32,
        kind: ReloadKind,
    ) -> Result<ModuleDto, V2Error> {
        let game_key = self.require_wasm_mode(state, game_id).await?;

        let queries = q(state).await?;
        let row = queries
            .room_module_at_version(game_id, version)
            .await
            .map_err(V2Error::from)?;
        let Some(row) = row else {
            return Err(V2Error::not_found(format!(
                "Module version {version} not found."
            )));
        };

        queries
            .activate_room_module(game_id, version)
            .await
            .map_err(V2Error::from)?;

        let mut module = module_dto(row);
        module.is_active = true;
        Self::publish_reload(state, game_id, &game_key, &module, kind);
        Ok(module)
    }
    pub async fn delete_module(
        &self,
        state: &ApiState,
        game_id: Uuid,
        version: i32,
    ) -> Result<(), V2Error> {
        self.require_wasm_mode(state, game_id).await?;
        let flags = q(state).await?
            .room_module_active_flag(game_id, version)
            .await
            .map_err(V2Error::from)?;
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
        let deleted = q(state).await?
            .delete_room_module(game_id, version)
            .await
            .map_err(V2Error::from)?;
        for row in deleted {
            if let Some(key) = row.blob_key
                && let Err(err) = blobs(state).await?.delete(&key).await
            {
                tracing::warn!("[wasm] orphaned blob {key}: {err}");
            }
        }
        Ok(())
    }
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
}
