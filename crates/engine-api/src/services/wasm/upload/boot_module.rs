use engine_config::BootModule;

use crate::services::deps::{blobs, q};
use crate::services::games::tenant;

use super::*;

impl WasmService {
    pub async fn load_boot_module(state: &ApiState) -> Option<BootModule> {
        match Self::try_load_boot_module(state).await {
            Ok(module) => module,
            Err(err) => {
                tracing::warn!(
                    "[wasm] could not load the console module at boot: {}",
                    err.message
                );
                None
            }
        }
    }

    async fn try_load_boot_module(state: &ApiState) -> Result<Option<BootModule>, V2Error> {
        let Some(game_id) = tenant::game_id(state).await? else {
            return Ok(None);
        };
        let Some(row) = q(state).await?
            .active_room_module(game_id)
            .await
            .map_err(V2Error::from)?
        else {
            return Ok(None);
        };
        let Some(key) = row.blob_key else {
            tracing::warn!("[wasm] active module v{} has no stored bytes", row.version);
            return Ok(None);
        };
        let Some(bytes) = blobs(state).await?
            .get(&key)
            .await
            .map_err(V2Error::from)?
        else {
            tracing::warn!("[wasm] {key} is missing from the blob store");
            return Ok(None);
        };
        Ok(Some(BootModule {
            version: row.version,
            sha256: row.sha256,
            bytes,
        }))
    }
}
