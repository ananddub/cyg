//! Auto-registered bindings for `/v2/console/games` — the games an operator registers,
//! inspects and retires.
//!
//! Transport only, like the rest of the console: take the token and the body, hand them
//! to [`GamesService`], return whatever it shapes. Registering a game and archiving one
//! are admin actions — a new tenant is a new credential, and an archive takes a live
//! game's rooms out of rotation — while reading the list is not. The one reply that is
//! not JSON is the Unity settings download, which answers as an attached file (or as
//! its JSON companion under `?format=json`).

use auto_route::controller;
use axum::Json;
use axum::extract::{Path, Query};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::{Body, BoolFlag};
use crate::current_state;
use crate::dto::console::games::{
    ArchiveGameResponse, CreateGameRequest, UnityConnectionDto, UpdateGameRequest,
};
use crate::error::V2Error;
use crate::services::games::GamesService;

/// Aliases so `auto_route` does not require extra `Type` bounds from plain `serde`
/// shapes; extraction and rejection are exactly `axum`'s.
type QueryParams<T> = axum::extract::Query<T>;

/// Wire protocol revision the SDK speaks (`CYGServerSettings.protocolVersion`).
const PROTOCOL_VERSION: u16 = 1;

/// Matchmake method the client connects with (`DefaultMatchmakeMethod`).
const MATCHMAKE_METHOD: &str = "joinOrCreate";

/// Guid Unity records for `CYGServerSettings.cs` in its `.meta`. The SDK does not
/// commit meta files, so a project that imports the package mints its own; the asset
/// carries this stable value and Unity re-binds the script on first import of the
/// asset into a project that contains the class.
const SERVER_SETTINGS_SCRIPT_GUID: &str = "d4c1b9a37e5f4a86b0c2d3e4f5061728";

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

    /// One game, secret included. Read rather than write: the operator who can reach
    /// the console already holds the game's key on screen.
    #[get("/{gameId}")]
    async fn get(&self, _auth: ConsoleAuthed, Path(game_id): Path<String>) -> Response {
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

    /// Retire a game. A soft delete: the row, its modules and its key stay, so the
    /// history of a game nobody plays is still readable.
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

    /// `GET /v2/console/games/{gameId}/unity-settings` — the Unity server-settings
    /// download. `?format=asset` (the default) returns the `CYGServerSettings.asset`
    /// body as an attachment; `?format=json` returns the connection companion, which
    /// is also the only place the game's credentials appear — the SDK class declares
    /// no serialized fields for them, so putting them in the asset would have Unity
    /// drop them. `?reveal=true` includes the `api_key` in the JSON reply.
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

/// Which of the two answers the unity-settings route gives.
enum UnityFormat {
    Asset,
    Json,
}

/// The query of `GET /v2/console/games/{gameId}/unity-settings`; both flags are
/// optional and a bare `?reveal=` or a hostile value is refused by `BoolFlag::parse`
/// rather than silently read.
#[derive(Debug, Default, Deserialize)]
struct UnitySettingsQuery {
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    reveal: Option<String>,
}

/// The `.asset` answer: the ScriptableObject YAML Unity deserializes into
/// `CookYourGame.CYGServerSettings`, attached under its canonical file name. The
/// field names below are exactly the `[SerializeField]` declarations of that class —
/// `serverHttpUrl`, `serverWsPath`, `roomName`, `maxPlayers`, `protocolVersion`,
/// `matchmakeMethod` — and nothing else, because Unity silently drops fields the
/// class does not declare.
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

/// The asset document, in the shape Unity's own serializer emits for a
/// `ScriptableObject` of `fileID` 114.
fn unity_asset_yaml(connection: &UnityConnectionDto) -> String {
    format!(
        r#"%YAML 1.1
%TAG !u! tag:unity3d.com,2011:
--- !u!114 &11400000
MonoBehaviour:
  m_ObjectHideFlags: 0
  m_CorrespondingSourceObject: {{fileID: 0}}
  m_PrefabInstance: {{fileID: 0}}
  m_PrefabAsset: {{fileID: 0}}
  m_GameObject: {{fileID: 0}}
  m_Enabled: 1
  m_EditorHideFlags: 0
  m_Script: {{fileID: 11500000, guid: {SERVER_SETTINGS_SCRIPT_GUID}, type: 3}}
  m_Name: CYGServerSettings
  serverHttpUrl: {server_http_url}
  serverWsPath: {server_ws_path}
  roomName: {room_name}
  maxPlayers: {max_players}
  protocolVersion: {PROTOCOL_VERSION}
  matchmakeMethod: {MATCHMAKE_METHOD}
"#,
        server_http_url = connection.server_http_url,
        server_ws_path = connection.server_ws_path,
        room_name = connection.default_room,
        max_players = connection.max_players,
    )
}

/// Parse a path id, naming the thing that was malformed the way the rest of the
/// console's failures do.
fn parse_id(raw: &str) -> Result<Uuid, V2Error> {
    Uuid::parse_str(raw).map_err(|_| V2Error::invalid_argument("Invalid game id."))
}

/// The console's one reply shape: a service value becomes JSON, a service failure
/// becomes its own status and envelope.
fn answer<T: Serialize>(result: Result<T, V2Error>) -> Response {
    match result {
        Ok(value) => Json(value).into_response(),
        Err(err) => err.into_response(),
    }
}
