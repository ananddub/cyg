use crate::services::deps::{blobs, q};

use super::*;

impl MapService {
    pub async fn list_maps(&self, state: &ApiState, game_id: Uuid) -> Result<Vec<MapDto>, V2Error> {
        let rows: Vec<ListGameMapsRow> = q(state).await?
            .list_game_maps(game_id)
            .await
            .map_err(V2Error::from)?;
        Ok(rows.into_iter().map(map_dto).collect())
    }
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

        let queries = q(state).await?;
        let version = queries
            .latest_game_map_version(game_id)
            .await
            .map_err(V2Error::from)?
            .unwrap_or(0)
            + 1;

        let now = Utc::now();
        let id = Uuid::new_v4();
        let key = map_key(game_id, version, format.as_str());
        blobs(state).await?.put(&key, &bytes).await.map_err(V2Error::from)?;
        let data = json!({
            "game_id": game_id.to_string(),
            "version": version,
            "name": name.clone(),
            "format": format.as_str(),
            "blob_key": key.clone(),
            "size_bytes": bytes.len() as i64,
            "sha256": sha256.clone(),
            "is_active": true,
            "created_at": time::encode(now),
        });
        if let Err(err) = queries
            .insert_game_map(game_id, id, format.as_str(), &data)
            .await
        {
            let _ = blobs(state).await?.delete(&key).await;
            return Err(V2Error::from(err));
        }

        let map = MapDto {
            id,
            version,
            name,
            format: format.as_str().to_string(),
            sha256,
            is_active: true,
            size_bytes: bytes.len() as i64,
            created_at: now,
        };
        Self::publish_reload(state, &game_key, &map, &bytes);
        Ok(MapUploadResponse { map })
    }
    pub async fn activate_map(
        &self,
        state: &ApiState,
        game_id: Uuid,
        version: i32,
    ) -> Result<MapDto, V2Error> {
        let game_key = crate::services::games::GamesService::find_game(state, game_id)
            .await?
            .game_key;
        let queries = q(state).await?;

        let row: Option<StoredGameMapRow> = queries
            .game_map_at_version(game_id, version)
            .await
            .map_err(V2Error::from)?;
        let Some(row) = row else {
            return Err(V2Error::not_found(format!(
                "Map version {version} not found."
            )));
        };

        queries
            .activate_game_map(game_id, version, &row.format)
            .await
            .map_err(V2Error::from)?;

        let map = MapDto {
            id: row.id,
            version: row.version,
            name: row.name,
            format: row.format,
            sha256: row.sha256,
            is_active: true,
            size_bytes: row.size_bytes,
            created_at: row.created_at,
        };
        let Some(key) = row.blob_key else {
            return Err(V2Error::not_found(format!(
                "Map version {version} bytes are missing."
            )));
        };
        let bytes = blobs(state).await?
            .get(&key)
            .await
            .map_err(V2Error::from)?
            .ok_or_else(|| {
                V2Error::not_found(format!("Map version {version} bytes are missing."))
            })?;
        Self::publish_reload(state, &game_key, &map, &bytes);
        Ok(map)
    }
}
