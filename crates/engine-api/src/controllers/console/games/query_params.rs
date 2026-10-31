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
        answer(GamesService::update_game(&state, id, &body).await)
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
