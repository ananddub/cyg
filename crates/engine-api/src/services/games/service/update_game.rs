use crate::services::deps::q;

use super::*;

impl GamesService {
    pub async fn update_game(
        state: &ApiState,
        game_id: Uuid,
        body: &UpdateGameRequest,
        author: Option<&str>,
    ) -> Result<GameDto, V2Error> {
        let patch_name = body.name.as_deref().map(game_name).transpose()?;
        let patch_engine = body.engine_type.as_deref().map(engine_type).transpose()?;
        let patch_status = body.status.as_deref().map(game_status).transpose()?;
        if patch_name.is_none()
            && patch_engine.is_none()
            && patch_status.is_none()
            && body.config.is_none()
            && body.tick_rate.is_none()
            && body.max_clients.is_none()
            && body.persistent_rooms.is_none()
            && body.auto_close_seconds.is_none()
        {
            return Err(V2Error::invalid_argument(
                "A game update has to change at least one field.",
            ));
        }

        let current = Self::find_game(state, game_id).await?;

        let name = match patch_name {
            Some(name) => name,
            None => current.name.clone(),
        };
        let engine = match patch_engine {
            Some(engine) => engine,
            None => current.engine_type.clone(),
        };
        let backend = backend_mode_for_engine(&engine);
        let status = match patch_status {
            Some(status) => status,
            None => current.status.clone(),
        };

        let mut map = current.config.as_object().cloned().unwrap_or_default();
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
        let config = serde_json::Value::Object(map);

        let row = q(state)
            .await?
            .update_game(&UpdateGameParams {
                id: game_id,
                name,
                engine_type: engine,
                status,
                config: config.clone(),
                backend_mode: backend.to_owned(),
            })
            .await
            .map_err(V2Error::from)?;

        Self::record_config_versions(state, game_id, &current.config, &config, author).await;

        let tick_rate = config
            .get("tick_rate")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);
        if let Some(tick_rate) = tick_rate {
            if crate::services::games::tenant::game_id(state)
                .await
                .ok()
                .flatten()
                .is_some_and(|hosted| hosted == game_id)
            {
                let _ = state
                    .bus
                    .settings_tx
                    .try_send(engine_bus::SettingsReload { tick_rate });
            }
        }

        Ok(GameDto::from(GameRow::from(row)))
    }
    pub async fn archive_game(state: &ApiState, game_id: Uuid) -> Result<bool, V2Error> {
        Self::find_game(state, game_id).await?;
        let done = q(state)
            .await?
            .archive_game(game_id)
            .await
            .map_err(V2Error::from)?;
        Ok(done.rows_affected() > 0)
    }
    pub async fn find_game(state: &ApiState, game_id: Uuid) -> Result<GameRow, V2Error> {
        q(state)
            .await?
            .find_game_by_id(game_id)
            .await
            .map_err(V2Error::from)?
            .into_iter()
            .next()
            .map(GameRow::from)
            .ok_or_else(|| V2Error::not_found(NOT_FOUND))
    }
    pub async fn find_game_by_key(
        state: &ApiState,
        game_key: &str,
    ) -> Result<Option<GameRow>, V2Error> {
        Ok(q(state)
            .await?
            .find_game_by_key(game_key)
            .await
            .map_err(V2Error::from)?
            .into_iter()
            .next()
            .map(GameRow::from))
    }
    pub async fn verify_api_key(
        state: &ApiState,
        game_key: &str,
        api_key: &str,
    ) -> Result<bool, V2Error> {
        let Some(stored) = q(state)
            .await?
            .find_game_secret_by_key(game_key)
            .await
            .map_err(V2Error::from)?
            .into_iter()
            .next()
        else {
            return Ok(false);
        };
        Ok(secrets_match(stored.as_bytes(), api_key.as_bytes()))
    }
    pub async fn active_module_version(
        state: &ApiState,
        game_id: Uuid,
    ) -> Result<Option<i32>, V2Error> {
        Ok(q(state)
            .await?
            .active_room_module_version(game_id)
            .await
            .map_err(V2Error::from)?
            .into_iter()
            .next())
    }
    pub async fn module_count(state: &ApiState, game_id: Uuid) -> Result<i64, V2Error> {
        q(state)
            .await?
            .count_room_modules(game_id)
            .await
            .map_err(V2Error::from)
    }
}
