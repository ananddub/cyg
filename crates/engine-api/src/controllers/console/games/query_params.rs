use super::*;

type QueryParams<T> = axum::extract::Query<T>;
pub(super) const PROTOCOL_VERSION: u16 = 1;
pub(super) const MATCHMAKE_METHOD: &str = "joinOrCreate";
pub(super) const SERVER_SETTINGS_SCRIPT_GUID: &str = "d4c1b9a37e5f4a86b0c2d3e4f5061728";
struct ConsoleGamesController;
#[controller("/v2/console/games")]
impl ConsoleGamesController {
    fn new() -> Self {
        Self
    }

    #[get("")]
    async fn list(&self, _auth: ConsoleAuthed) -> Response {
        let state = current_state();
        answer(GamesService::list_games(&state).await)
    }

    #[post("")]
    async fn create(&self, auth: ConsoleAuthed, Body(body): Body<CreateGameRequest>) -> Response {
        let state = current_state();
        if let Err(err) = auth.require_admin() {
            return err.into_response();
        }
        answer(GamesService::create_game(&state, &body).await)
    }

    #[get("/{gameId}")]
    pub(super) async fn get(&self, _auth: ConsoleAuthed, Path(game_id): Path<String>) -> Response {
        let state = current_state();
        let id = match parse_id(&game_id) {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        answer(GamesService::get_game(&state, id).await)
    }

    #[put("/{gameId}")]
    async fn update(
        &self,
        auth: ConsoleAuthed,
        Path(game_id): Path<String>,
        Body(body): Body<UpdateGameRequest>,
    ) -> Response {
        let state = current_state();
        if let Err(err) = auth.require_write() {
            return err.into_response();
        }
        let id = match parse_id(&game_id) {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        let updated = GamesService::update_game(&state, id, &body, Some(&auth.0.username)).await;
        if updated.is_ok() {
            config_audit(
                &state,
                &auth,
                id,
                "game.config.update",
                "Updated the game config.",
            )
            .await;
        }
        answer(updated)
    }

    #[get("/{gameId}/config/history")]
    async fn config_history(&self, _auth: ConsoleAuthed, Path(game_id): Path<String>) -> Response {
        let state = current_state();
        let id = match parse_id(&game_id) {
            Ok(id) => id,
            Err(err) => return err.into_response(),
        };
        answer(GamesService::config_history(&state, id).await)
    }

    #[get("/{gameId}/config/{version}")]
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
        answer(GamesService::config_version(&state, id, version).await)
    }

    #[post("/{gameId}/config/rollback/{version}")]
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
        answer(GamesService::rollback_config(&state, id, version, operator).await)
    }

    #[delete("/{gameId}")]
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
            GamesService::archive_game(&state, id)
                .await
                .map(|archived| ArchiveGameResponse { archived }),
        )
    }

    #[get("/{gameId}/unity-settings")]
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
        let connection = match GamesService::unity_settings(&state, id, reveal, &headers).await {
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
    let Ok(queries) = crate::services::deps::q(state).await else {
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
