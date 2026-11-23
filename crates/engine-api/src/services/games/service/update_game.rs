use super::*;

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

    pub async fn archive_game(&self, game_id: Uuid) -> Result<bool, V2Error> {
        Self::find_game(self, game_id).await?;
        let done = self
            .plane
            .q()
            .await?
            .archive_game(game_id)
            .await
            .map_err(V2Error::from)?;
        Ok(done.rows_affected() > 0)
    }
    /// Batch read (`API Design Patterns`, ch. 18): every id must exist, and the
    /// reply keeps the caller's order.
    pub async fn batch_get(&self, ids: &[Uuid]) -> Result<Vec<GameDto>, V2Error> {
        if ids.is_empty() {
            return Err(V2Error::invalid_argument(
                "A batch request must name at least one game.",
            ));
        }
        if ids.len() > 100 {
            return Err(V2Error::invalid_argument(
                "A batch request names at most 100 games.",
            ));
        }
        let mut games = Vec::with_capacity(ids.len());
        for id in ids {
            games.push(self.find_game(*id).await.map(GameDto::from)?);
        }
        Ok(games)
    }

    /// Batch archive. Missing ids are refused before anything is written, so a
    /// typo cannot leave a partially archived batch.
    pub async fn batch_archive(&self, ids: &[Uuid]) -> Result<usize, V2Error> {
        if ids.is_empty() {
            return Err(V2Error::invalid_argument(
                "A batch request must name at least one game.",
            ));
        }
        if ids.len() > 100 {
            return Err(V2Error::invalid_argument(
                "A batch request names at most 100 games.",
            ));
        }
        let mut unique = Vec::new();
        for id in ids {
            if !unique.contains(id) {
                unique.push(*id);
            }
        }
        for id in &unique {
            Self::find_game(self, *id).await?;
        }
        for id in &unique {
            self.archive_game(*id).await?;
        }
        Ok(unique.len())
    }

    /// Criteria-based archive (`API Design Patterns`, ch. 19). Empty criteria
    /// are refused so a missing filter cannot archive the fleet.
    pub async fn archive_matching(
        &self,
        status: Option<&str>,
        name_contains: Option<&str>,
    ) -> Result<usize, V2Error> {
        let status = status.map(game_status).transpose()?;
        let name_contains = name_contains
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if let Some(value) = name_contains {
            if value.len() > 128 {
                return Err(V2Error::invalid_argument(
                    "A name filter is at most 128 characters.",
                ));
            }
        }
        if status.is_none() && name_contains.is_none() {
            return Err(V2Error::invalid_argument(
                "A criteria delete needs at least one filter.",
            ));
        }
        let done = self
            .plane
            .q()
            .await?
            .archive_games_matching(status.as_deref(), name_contains)
            .await
            .map_err(V2Error::from)?;
        Ok(done.rows_affected() as usize)
    }

    /// Soft-delete restore (`API Design Patterns`, ch. 25). The row and its
    /// tenant key survive an archive; undelete only moves the state back.
    pub async fn restore_game(&self, game_id: Uuid) -> Result<GameDto, V2Error> {
        Self::find_game(self, game_id).await?;
        let done = self
            .plane
            .q()
            .await?
            .restore_game(game_id)
            .await
            .map_err(V2Error::from)?;
        if done.rows_affected() == 0 {
            return Err(V2Error::failed_precondition(
                "This game is not archived, so there is nothing to undelete.",
            ));
        }
        self.find_game(game_id).await.map(GameDto::from)
    }

    pub async fn find_game(&self, game_id: Uuid) -> Result<GameRow, V2Error> {
        self.plane
            .q()
            .await?
            .find_game_by_id(game_id)
            .await
            .map_err(V2Error::from)?
            .into_iter()
            .next()
            .map(GameRow::from)
            .ok_or_else(|| V2Error::not_found(NOT_FOUND))
    }
    pub async fn find_game_by_key(&self, game_key: &str) -> Result<Option<GameRow>, V2Error> {
        Ok(self
            .plane
            .q()
            .await?
            .find_game_by_key(game_key)
            .await
            .map_err(V2Error::from)?
            .into_iter()
            .next()
            .map(GameRow::from))
    }
    pub async fn verify_api_key(&self, game_key: &str, api_key: &str) -> Result<bool, V2Error> {
        let Some(stored) = self
            .plane
            .q()
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
    pub async fn active_module_version(&self, game_id: Uuid) -> Result<Option<i32>, V2Error> {
        Ok(self
            .plane
            .q()
            .await?
            .active_room_module_version(game_id)
            .await
            .map_err(V2Error::from)?
            .into_iter()
            .next())
    }
    pub async fn module_count(&self, game_id: Uuid) -> Result<i64, V2Error> {
        self.plane
            .q()
            .await?
            .count_room_modules(game_id)
            .await
            .map_err(V2Error::from)
    }
}

/// Fields the update mask may name. The flattened spellings are accepted
/// because the dashboard already uses them; both map to the canonical config
/// key so a mask and the body cannot disagree about what changed.
const UPDATE_MASK_FIELDS: &[&str] = &[
    "name",
    "engine_type",
    "status",
    "config",
    "tick_rate",
    "max_clients",
    "persistent_rooms",
    "auto_close_seconds",
    "config.tick_rate",
    "config.max_clients",
    "config.persistent_rooms",
    "config.auto_close_seconds",
];

#[derive(Debug, Clone)]
pub(super) struct GameUpdatePlan {
    name: String,
    engine_type: String,
    backend_mode: String,
    status: String,
    config: serde_json::Value,
}

fn preview(current: &GameRow, plan: &GameUpdatePlan) -> GameDto {
    GameDto {
        id: current.id,
        game_key: current.game_key.clone(),
        name: plan.name.clone(),
        client_key: current.client_key.clone(),
        engine_type: plan.engine_type.clone(),
        backend_mode: plan.backend_mode.clone(),
        status: plan.status.clone(),
        config: plan.config.clone(),
        created_at: current.created_at,
        updated_at: current.updated_at,
    }
}

/// Apply the explicit mask to the body. A field named by the mask must carry a
/// value; a field the body carries but the mask does not name is ignored. An
/// absent mask keeps the book's implicit mask (the fields present in the body).
fn masked_update(
    body: &UpdateGameRequest,
    mask: Option<&[String]>,
) -> Result<UpdateGameRequest, V2Error> {
    let Some(mask) = mask else {
        return Ok(body.clone());
    };
    if mask.is_empty() {
        return Err(V2Error::invalid_argument(
            "An updateMask must name at least one field.",
        ));
    }
    let mut out = UpdateGameRequest::default();
    for raw in mask {
        let field = raw.trim();
        if !UPDATE_MASK_FIELDS.contains(&field) {
            return Err(V2Error::invalid_argument(format!(
                "Unknown updateMask field '{field}'."
            )));
        }
        match field {
            "name" => out.name = Some(required(body.name.clone(), field)?),
            "engine_type" => out.engine_type = Some(required(body.engine_type.clone(), field)?),
            "status" => out.status = Some(required(body.status.clone(), field)?),
            "config" => out.config = Some(required(body.config.clone(), field)?),
            "tick_rate" | "config.tick_rate" => {
                let value = body.tick_rate.or_else(|| {
                    body.config
                        .as_ref()
                        .and_then(|config| config.get("tick_rate"))
                        .and_then(|value| value.as_u64())
                        .map(|value| value as u32)
                });
                out.tick_rate = Some(required(value, field)?);
            }
            "max_clients" | "config.max_clients" => {
                let value = body.max_clients.or_else(|| {
                    body.config
                        .as_ref()
                        .and_then(|config| config.get("max_clients"))
                        .and_then(|value| value.as_u64())
                        .map(|value| value as u32)
                });
                out.max_clients = Some(required(value, field)?);
            }
            "persistent_rooms" | "config.persistent_rooms" => {
                let value = body.persistent_rooms.or_else(|| {
                    body.config
                        .as_ref()
                        .and_then(|config| config.get("persistent_rooms"))
                        .and_then(|value| value.as_bool())
                });
                out.persistent_rooms = Some(required(value, field)?);
            }
            "auto_close_seconds" | "config.auto_close_seconds" => {
                let value = body.auto_close_seconds.or_else(|| {
                    body.config
                        .as_ref()
                        .and_then(|config| config.get("auto_close_seconds"))
                        .and_then(|value| value.as_u64())
                        .map(|value| value as u32)
                });
                out.auto_close_seconds = Some(required(value, field)?);
            }
            _ => unreachable!("the mask was checked against UPDATE_MASK_FIELDS"),
        }
    }
    Ok(out)
}

fn required<T>(value: Option<T>, field: &str) -> Result<T, V2Error> {
    value.ok_or_else(|| {
        V2Error::invalid_argument(format!(
            "The updateMask names '{field}', but the request does not set it."
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::INVALID_ARGUMENT;

    fn body(value: serde_json::Value) -> UpdateGameRequest {
        serde_json::from_value(value).expect("update body")
    }

    fn mask(fields: &[&str]) -> Vec<String> {
        fields.iter().map(|field| (*field).to_owned()).collect()
    }

    #[test]
    fn an_explicit_mask_updates_only_the_fields_it_names() {
        let request = body(json!({
            "name": "Renamed",
            "status": "suspended",
            "tick_rate": 20,
        }));
        let filtered = masked_update(&request, Some(&mask(&["name"]))).expect("mask applies");
        assert_eq!(filtered.name.as_deref(), Some("Renamed"));
        assert_eq!(filtered.status, None, "a field outside the mask is ignored");
        assert_eq!(filtered.tick_rate, None, "and so is the flattened config");
    }

    #[test]
    fn a_mask_that_names_a_field_the_body_does_not_set_is_refused() {
        let request = body(json!({"name": "Renamed"}));
        let err = masked_update(&request, Some(&mask(&["status"]))).expect_err("missing value");
        assert_eq!(err.kind, INVALID_ARGUMENT, "{err:?}");
        assert!(err.message.contains("status"), "{err:?}");
    }

    #[test]
    fn a_flat_config_mask_reads_the_nested_config_value() {
        let request = body(json!({"config": {"tick_rate": 90}}));
        let filtered = masked_update(&request, Some(&mask(&["config.tick_rate"])))
            .expect("nested mask applies");
        assert_eq!(
            filtered.tick_rate,
            Some(90),
            "the flat spelling is a view of the same config key"
        );
    }

    #[test]
    fn an_empty_or_unknown_mask_is_refused() {
        let request = body(json!({"name": "Renamed"}));
        let empty = masked_update(&request, Some(&[])).expect_err("empty mask");
        assert_eq!(empty.kind, INVALID_ARGUMENT, "{empty:?}");
        let unknown = masked_update(&request, Some(&mask(&["owner"]))).expect_err("unknown field");
        assert_eq!(unknown.kind, INVALID_ARGUMENT, "{unknown:?}");
        assert!(unknown.message.contains("owner"), "{unknown:?}");
    }
}
