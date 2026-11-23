use super::*;

mod tests;
mod update_game_batch;
mod update_game_lookup;
mod update_game_mask;
use update_game_mask::*;

impl GamesService {
    /// Whole-resource update retained for clients that already send the full
    /// game document. New callers should use [`Self::update_game_with_mask`].
    pub async fn update_game(
        &self,
        game_id: Uuid,
        body: &UpdateGameRequest,
        author: Option<&str>,
    ) -> Result<GameDto, V2Error> {
        self.update_game_with_mask(game_id, body, author, None)
            .await
    }

    /// Partial update with the book's implicit-or-explicit field-mask rule
    /// (`API Design Patterns`, ch. 8). An absent mask updates the fields the
    /// body names; a present mask updates exactly those fields and ignores
    /// everything else. `validateOnly` plans the write and returns the result
    /// without touching the row (`API Design Patterns`, ch. 27).
    pub async fn update_game_with_mask(
        &self,
        game_id: Uuid,
        body: &UpdateGameRequest,
        author: Option<&str>,
        mask: Option<&[String]>,
    ) -> Result<GameDto, V2Error> {
        let masked = masked_update(body, mask)?;
        let validate_only = body.validate_only.unwrap_or(false);
        let (current, plan) = self.plan_update(game_id, &masked).await?;
        if validate_only {
            return Ok(preview(&current, &plan));
        }
        self.commit_update(&current, &plan, author).await
    }

    async fn plan_update(
        &self,
        game_id: Uuid,
        body: &UpdateGameRequest,
    ) -> Result<(GameRow, GameUpdatePlan), V2Error> {
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

        let current = Self::find_game(self, game_id).await?;
        let name = patch_name.unwrap_or_else(|| current.name.clone());
        let engine = patch_engine.unwrap_or_else(|| current.engine_type.clone());
        let status = patch_status.unwrap_or_else(|| current.status.clone());
        let mut map = current.config.as_object().cloned().unwrap_or_default();
        if let Some(patch) = config_value(body.config.as_ref())? {
            if let Some(patch_map) = patch.as_object() {
                for (key, value) in patch_map {
                    map.insert(key.clone(), value.clone());
                }
            }
        }
        for (key, value) in [
            ("tick_rate", body.tick_rate.map(|v| json!(v))),
            ("max_clients", body.max_clients.map(|v| json!(v))),
            ("persistent_rooms", body.persistent_rooms.map(|v| json!(v))),
            (
                "auto_close_seconds",
                body.auto_close_seconds.map(|v| json!(v)),
            ),
        ] {
            if let Some(value) = value {
                map.insert(key.to_owned(), value);
            }
        }
        Ok((
            current,
            GameUpdatePlan {
                name,
                engine_type: engine.clone(),
                backend_mode: backend_mode_for_engine(&engine).to_owned(),
                status,
                config: serde_json::Value::Object(map),
            },
        ))
    }

    async fn commit_update(
        &self,
        current: &GameRow,
        plan: &GameUpdatePlan,
        author: Option<&str>,
    ) -> Result<GameDto, V2Error> {
        let row = self
            .plane
            .q()
            .await?
            .update_game(&UpdateGameParams {
                id: current.id,
                name: plan.name.clone(),
                engine_type: plan.engine_type.clone(),
                status: plan.status.clone(),
                config: plan.config.clone(),
                backend_mode: plan.backend_mode.clone(),
            })
            .await
            .map_err(V2Error::from)?;

        Self::record_config_versions(self, current.id, &current.config, &plan.config, author).await;

        let tick_rate = plan
            .config
            .get("tick_rate")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);
        if let Some(tick_rate) = tick_rate {
            if crate::services::games::tenant::game_id(&self.plane)
                .await
                .ok()
                .flatten()
                .is_some_and(|hosted| hosted == current.id)
            {
                let _ = self
                    .plane
                    .bus
                    .settings_tx
                    .try_send(engine_bus::SettingsReload { tick_rate });
            }
        }

        Ok(GameDto::from(GameRow::from(row)))
    }
}
