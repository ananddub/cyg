use super::*;
use std::sync::Arc;

type QueryParams<T> = axum::extract::Query<T>;
pub(super) const PROTOCOL_VERSION: u16 = 1;
pub(super) const MATCHMAKE_METHOD: &str = "joinOrCreate";
pub(super) const SERVER_SETTINGS_SCRIPT_GUID: &str = "d4c1b9a37e5f4a86b0c2d3e4f5061728";
struct ConsoleGamesController {
    service: Arc<GamesService>,
}
type ConfigVersionRowList = Vec<crate::dto::console::config_history::ConfigVersionRow>;
#[controller("/v1/console/games")]
impl ConsoleGamesController {
    fn new(service: Arc<GamesService>) -> Self {
        Self { service }
    }

    #[get("", response_body = crate::dto::console::games::GameListDto)]
    async fn list(&self, _auth: ConsoleAuthed) -> Response {
        let state = current_state();
        answer(self.service.list_games().await)
    }

    #[post("", response_body = crate::dto::console::games::GameDto)]
    async fn create(&self, auth: ConsoleAuthed, Body(body): Body<CreateGameRequest>) -> Response {
        let state = current_state();
        if let Err(err) = auth.require_admin() {
            return err.into_response();
        }
        answer(self.service.create_game(&body).await)
    }

    #[get("/{gameId}", response_body = crate::dto::console::games::GameDetailDto)]
    pub(super) async fn get(&self, _auth: ConsoleAuthed, Path(game_id): Path<String>) -> Response {
        let state = current_state();
        let id = match parse_id(&game_id) {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        answer(self.service.get_game(id).await)
    }

    #[put("/{gameId}")]
    async fn update(
        &self,
        auth: ConsoleAuthed,
        Path(game_id): Path<String>,
        Json(body): Json<UpdateGameRequest>,
    ) -> Result<Json<GameDto>, V2Error> {
        self.apply_update(auth, game_id, body, None).await
    }

    /// Standard partial update (`API Design Patterns`, ch. 8). The mask is a
    /// comma-separated field list; when it is absent the fields present in the
    /// body are the implicit mask.
    #[patch("/{gameId}")]
    async fn patch(
        &self,
        auth: ConsoleAuthed,
        Path(game_id): Path<String>,
        Query(query): QueryParams<UpdateGameQuery>,
        Json(body): Json<UpdateGameRequest>,
    ) -> Result<Json<GameDto>, V2Error> {
        let mask = parse_update_mask(query.update_mask.as_deref())?;
        self.apply_update(auth, game_id, body, mask.as_deref())
            .await
    }

    /// Custom method on the resource (`API Design Patterns`, ch. 9): restore a
    /// soft-deleted game. The colon keeps the action attached to the resource
    /// instead of inventing a child path.
    /// Canonical URL: `POST /v1/console/games/{id}:undelete`. The router
    /// adapter rewrites the colon form to this nested handler.
    #[post("/{gameId}/undelete")]
    async fn undelete(
        &self,
        auth: ConsoleAuthed,
        Path(game_id): Path<String>,
    ) -> Result<Json<GameDto>, V2Error> {
        let state = current_state();
        auth.require_write()?;
        let id = parse_id(&game_id)?;
        let restored = self.service.restore_game(id).await?;
        config_audit(
            &state,
            &auth,
            id,
            "game.config.undelete",
            "Restored an archived game.",
        )
        .await;
        Ok(Json(restored))
    }

    async fn apply_update(
        &self,
        auth: ConsoleAuthed,
        game_id: String,
        body: UpdateGameRequest,
        mask: Option<&[String]>,
    ) -> Result<Json<GameDto>, V2Error> {
        let state = current_state();
        auth.require_write()?;
        let id = parse_id(&game_id)?;
        let validate_only = body.validate_only.unwrap_or(false);
        let updated = self
            .service
            .update_game_with_mask(id, &body, Some(&auth.0.username), mask)
            .await?;
        if !validate_only {
            config_audit(
                &state,
                &auth,
                id,
                "game.config.update",
                "Updated the game config.",
            )
            .await;
        }
        Ok(Json(updated))
    }

    /// Book batch get (`ch. 18`), targeting the collection:
    /// `POST /v1/console/games:batch-get`.
    #[post("/batch-get")]
    async fn batch_get(
        &self,
        _auth: ConsoleAuthed,
        Json(request): Json<GameIdsRequest>,
    ) -> Result<Json<GameBatchResponse>, V2Error> {
        let games = self.service.batch_get(&request.ids).await?;
        Ok(Json(GameBatchResponse { games }))
    }

    /// Book batch delete (`ch. 18`). Missing ids are refused before any write.
    /// `POST /v1/console/games:batch-delete`.
    #[post("/batch-delete")]
    async fn batch_delete(
        &self,
        auth: ConsoleAuthed,
        Json(request): Json<GameIdsRequest>,
    ) -> Result<Json<GameArchiveBatchResponse>, V2Error> {
        let state = current_state();
        auth.require_admin()?;
        let archived = self.service.batch_archive(&request.ids).await?;
        for id in &request.ids {
            config_audit(
                &state,
                &auth,
                *id,
                "game.config.batch_delete",
                "Archived a game in a batch delete.",
            )
            .await;
        }
        Ok(Json(GameArchiveBatchResponse { archived }))
    }

    /// Criteria-based delete (`API Design Patterns`, ch. 19).
    /// `POST /v1/console/games:delete-by-filter`.
    #[post("/delete-by-filter")]
    async fn delete_by_filter(
        &self,
        auth: ConsoleAuthed,
        Json(request): Json<GameCriteriaRequest>,
    ) -> Result<Json<GameArchiveBatchResponse>, V2Error> {
        auth.require_admin()?;
        let archived = self
            .service
            .archive_matching(request.status.as_deref(), request.name_contains.as_deref())
            .await?;
        Ok(Json(GameArchiveBatchResponse { archived }))
    }

    #[get("/{gameId}/config/history", response_body = ConfigVersionRowList)]
    async fn config_history(&self, _auth: ConsoleAuthed, Path(game_id): Path<String>) -> Response {
        let state = current_state();
        let id = match parse_id(&game_id) {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        answer(self.service.config_history(id).await)
    }

    #[get("/{gameId}/config/{version}", response_body = crate::dto::console::config_history::ConfigVersionRow)]
    async fn config_version(
        &self,
        _auth: ConsoleAuthed,
        Path((game_id, version)): Path<(String, String)>,
    ) -> Response {
        let state = current_state();
        let id = match parse_id(&game_id) {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        let Ok(version) = version.parse::<i64>() else {
            return V2Error::invalid_argument("A config version is a number.").into_response();
        };
        answer(self.service.config_version(id, version).await)
    }

    #[post("/{gameId}/config/rollback/{version}", response_body = crate::dto::console::config_history::ConfigVersionRow)]
    async fn rollback_config(
        &self,
        auth: ConsoleAuthed,
        Path((game_id, version)): Path<(String, String)>,
    ) -> Response {
        let state = current_state();
        if let Err(err) = auth.require_write() {
            return err.into_response();
        }
        let id = match parse_id(&game_id) {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        let Ok(version) = version.parse::<i64>() else {
            return V2Error::invalid_argument("A config version is a number.").into_response();
        };
        let operator = auth
            .0
            .uuid()
            .map(|id| (id, auth.0.username.clone(), auth.0.email.clone()));
        answer(self.service.rollback_config(id, version, operator).await)
    }

    #[delete("/{gameId}", response_body = crate::dto::console::games::ArchiveGameResponse)]
    async fn archive(&self, auth: ConsoleAuthed, Path(game_id): Path<String>) -> Response {
        let state = current_state();
        if let Err(err) = auth.require_admin() {
            return err.into_response();
        }
        let id = match parse_id(&game_id) {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        answer(
            self.service
                .archive_game(id)
                .await
                .map(|archived| ArchiveGameResponse { archived }),
        )
    }

    #[get("/{gameId}/unity-settings", response_body = crate::dto::console::games::UnityConnectionDto)]
    async fn unity_settings(
        &self,
        _auth: ConsoleAuthed,
        Path(game_id): Path<String>,
        Query(query): QueryParams<UnitySettingsQuery>,
        headers: HeaderMap,
    ) -> Response {
        let state = current_state();
        let format = match query.format.as_deref().unwrap_or("asset") {
            "asset" => UnityFormat::Asset,
            "json" => UnityFormat::Json,
            other => {
                return V2Error::invalid_argument(format!(
                    "Unknown unity-settings format '{other}'; expected asset or json."
                ))
                .into_response();
            }
        };
        let reveal = match query.reveal.as_deref() {
            None => false,
            Some(raw) => match BoolFlag::parse(raw) {
                Ok(flag) => flag.create(),
                Err(err) => return err.into_response(),
            },
        };
        let id = match parse_id(&game_id) {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        let connection = match self.service.unity_settings(id, reveal, &headers).await {
            Ok(connection) => connection,
            Err(err) => return err.into_response(),
        };
        match format {
            UnityFormat::Json => answer(Ok(connection)),
            UnityFormat::Asset => asset_response(&connection),
        }
    }
}
enum UnityFormat {
    Asset,
    Json,
}
#[derive(Debug, Default, Deserialize)]
struct UnitySettingsQuery {
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    reveal: Option<String>,
}
fn asset_response(connection: &UnityConnectionDto) -> Response {
    let yaml = unity_asset_yaml(connection);
    Response::builder()
        .status(StatusCode::OK)
        .header(axum::http::header::CONTENT_TYPE, "application/octet-stream")
        .header(
            axum::http::header::CONTENT_DISPOSITION,
            "attachment; filename=\"CYGServerSettings.asset\"",
        )
        .body(axum::body::Body::from(yaml))
        .unwrap_or_else(|_| {
            V2Error::internal("The asset reply could not be built.").into_response()
        })
}
/// The audit row a config write leaves behind, operator-attributed.
async fn config_audit(
    state: &crate::ApiState,
    auth: &ConsoleAuthed,
    game_id: uuid::Uuid,
    action: &str,
    message: &str,
) {
    let Some(operator) = auth.0.uuid() else {
        return;
    };
    let Ok(queries) = crate::services::deps::q(&*state.plane()).await else {
        return;
    };
    let _ = queries
        .insert_audit_log(
            &crate::repositories::console_mutations::InsertAuditLogParams {
                id: uuid::Uuid::new_v4(),
                console_user_id: operator,
                console_username: auth.0.username.clone(),
                email: auth.0.email.clone(),
                action: action.to_owned(),
                resource: "game".to_owned(),
                message: format!("{message} (game {game_id})"),
                metadata: serde_json::json!({ "game_id": game_id }),
            },
        )
        .await;
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateGameQuery {
    update_mask: Option<String>,
}

fn parse_update_mask(raw: Option<&str>) -> Result<Option<Vec<String>>, V2Error> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let fields: Vec<String> = raw
        .split(',')
        .map(str::trim)
        .filter(|field| !field.is_empty())
        .map(str::to_owned)
        .collect();
    if fields.is_empty() {
        return Err(V2Error::invalid_argument(
            "An updateMask must name at least one field.",
        ));
    }
    Ok(Some(fields))
}
