//! Auto-registered bindings for `/v2/console/games/{gameId}/maps*` — the map
//! assets of one game: upload (multipart, format-gated), list, activate, and
//! delete of a stored version. Transport only, like every console controller:
//! parse the multipart into bytes, hand the rest to [`MapService`].

use std::sync::Arc;

use auto_route::controller;
use axum::Json;
use axum::extract::{DefaultBodyLimit, FromRequest, Multipart, Path, Request};
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

use crate::app::middleware::ConsoleAuthed;
use crate::current_state;
use crate::error::V2Error;
use crate::services::maps::MapService;
use engine_config::MapFormat;

/// One uploaded map: a cooked mesh or a navmesh can reach a few MB.
const MAX_UPLOAD_BYTES: usize = 32 * 1024 * 1024;

/// What a stored name/file name can hold; longer input is refused, not
/// silently truncated.
const MAX_NAME_LEN: usize = 255;

/// The accepted file extensions per `game_maps.format`; used only when the
/// request does not name a format itself.
const EXT_FORMATS: &[(&str, &str)] = &[
    ("oxcl", "oxcl"),
    ("json", "map2d"),
    ("nav2d", "navmesh"),
    ("mesh", "navmesh"),
];

struct ConsoleMapsController {
    service: Arc<MapService>,
}

#[controller("/v2/console/games/{gameId}/maps")]
impl ConsoleMapsController {
    fn new(service: Arc<MapService>) -> Self {
        Self { service }
    }

    /// `POST …/maps/upload` — multipart with a required `file` field plus a
    /// `format` field (`oxcl` | `map2d` | `map3d` | `navmesh`); without a
    /// format the file's extension picks one. An optional `name` overrides the
    /// stored display name (default: the filename).
    #[post("/upload")]
    async fn upload(
        &self,
        auth: ConsoleAuthed,
        Path(game_id): Path<String>,
        request: Request,
    ) -> Response {
        self.store_upload(auth, game_id, request).await
    }

    /// `POST …/maps` — the same upload at the collection path.
    #[post("")]
    async fn upload_at_root(
        &self,
        auth: ConsoleAuthed,
        Path(game_id): Path<String>,
        request: Request,
    ) -> Response {
        self.store_upload(auth, game_id, request).await
    }

    /// `GET …/maps` — every stored version, newest first, bytes excluded.
    #[get("")]
    async fn list(&self, _auth: ConsoleAuthed, Path(game_id): Path<String>) -> Response {
        let state = current_state();
        let game_id = match Self::game_id(&game_id) {
            Ok(id) => id,
            Err(e) => return e.into_response(),
        };
        self.service.list_maps(&state, game_id).await.map(Json).into_response()
    }

    /// `POST …/maps/{version}/activate` — make a stored version the active map
    /// of its format; the host loop applies it at the next room spawn.
    #[post("/{version}/activate")]
    async fn activate(
        &self,
        auth: ConsoleAuthed,
        Path((game_id, version)): Path<(String, String)>,
    ) -> Response {
        let state = current_state();
        let parsed = auth
            .require_write()
            .and_then(|()| Self::game_id(&game_id))
            .and_then(|id| Self::version(&version).map(|v| (id, v)));
        let (game_id, version) = match parsed {
            Ok(pair) => pair,
            Err(e) => return e.into_response(),
        };
        self.service
            .activate_map(&state, game_id, version)
            .await
            .map(Json)
            .into_response()
    }

    /// `DELETE …/maps/{version}` — drop a stored version; the active one is
    /// refused so a boot can never point at deleted bytes.
    #[delete("/{version}")]
    async fn delete(
        &self,
        auth: ConsoleAuthed,
        Path((game_id, version)): Path<(String, String)>,
    ) -> Response {
        let state = current_state();
        let parsed = auth
            .require_write()
            .and_then(|()| Self::game_id(&game_id))
            .and_then(|id| Self::version(&version).map(|v| (id, v)));
        let (game_id, version) = match parsed {
            Ok(pair) => pair,
            Err(e) => return e.into_response(),
        };
        self.service
            .delete_map(&state, game_id, version)
            .await
            .into_response()
    }
}

impl ConsoleMapsController {
    /// Both POST shapes of the upload: parse, gate, store, publish.
    async fn store_upload(
        &self,
        auth: ConsoleAuthed,
        game_id: String,
        request: Request,
    ) -> Response {
        let state = current_state();
        let game_id = match auth.require_write().and_then(|()| Self::game_id(&game_id)) {
            Ok(id) => id,
            Err(e) => return e.into_response(),
        };
        let upload = match Self::read_upload(request).await {
            Ok(upload) => upload,
            Err(e) => return e.into_response(),
        };
        self.service
            .upload_map(&state, game_id, upload.name, upload.format, upload.bytes)
            .await
            .map(Json)
            .into_response()
    }

    /// Drain the multipart body into [`UploadParts`]. The router-wide request
    /// cap is lifted for this one request by inserting the `DefaultBodyLimit`
    /// extension its `Multipart` extractor reads — no global layer, no other
    /// route sees a bigger limit.
    async fn read_upload(mut request: Request) -> Result<UploadParts, V2Error> {
        request
            .extensions_mut()
            .insert(DefaultBodyLimit::max(MAX_UPLOAD_BYTES));
        let mut multipart = Multipart::from_request(request, &())
            .await
            .map_err(|e| V2Error::invalid_argument(format!("Invalid multipart upload: {e}")))?;

        let mut filename: Option<String> = None;
        let mut declared_format: Option<String> = None;
        let mut display_name: Option<String> = None;
        let mut bytes: Option<Vec<u8>> = None;
        while let Some(field) = multipart
            .next_field()
            .await
            .map_err(|e| V2Error::invalid_argument(format!("Multipart read failed: {e}")))?
        {
            match field.name().unwrap_or_default() {
                "file" => {
                    let name = field.file_name().unwrap_or("map.bin").to_owned();
                    if name.len() > MAX_NAME_LEN {
                        return Err(V2Error::invalid_argument(format!(
                            "The file name is too long (max {MAX_NAME_LEN} characters)."
                        )));
                    }
                    filename = Some(name);
                    bytes = Some(
                        field
                            .bytes()
                            .await
                            .map_err(|e| {
                                V2Error::invalid_argument(format!("Map read failed: {e}"))
                            })?
                            .to_vec(),
                    );
                }
                "format" => {
                    declared_format = Some(
                        field
                            .text()
                            .await
                            .map_err(|e| {
                                V2Error::invalid_argument(format!("Format read failed: {e}"))
                            })?
                            .trim()
                            .to_owned(),
                    );
                }
                "name" => {
                    let given = field
                        .text()
                        .await
                        .map_err(|e| V2Error::invalid_argument(format!("Name read failed: {e}")))?;
                    if given.len() > MAX_NAME_LEN {
                        return Err(V2Error::invalid_argument(format!(
                            "The map name is too long (max {MAX_NAME_LEN} characters)."
                        )));
                    }
                    display_name = Some(given);
                }
                _ => {}
            }
        }

        let bytes = bytes
            .ok_or_else(|| V2Error::invalid_argument("The upload needs a `file` field."))?;
        let filename = filename.unwrap_or_else(|| "map.bin".to_owned());
        let name = display_name.unwrap_or_else(|| filename.clone());
        let format = match declared_format.as_deref().filter(|f| !f.is_empty()) {
            Some(raw) => MapFormat::parse(raw).ok_or_else(|| {
                V2Error::invalid_argument(
                    "Unknown map format; use one of oxcl, map2d, map3d, navmesh.",
                )
            })?,
            None => {
                let ext = filename
                    .rsplit('.')
                    .next()
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                let format = EXT_FORMATS.iter().find(|(e, _)| *e == ext).map(|(_, f)| *f);
                MapFormat::parse(format.unwrap_or_default()).ok_or_else(|| {
                    V2Error::invalid_argument(
                        "Could not infer the map format from the file extension; \
                         send a `format` field (oxcl, map2d, map3d or navmesh).",
                    )
                })?
            }
        };
        Ok(UploadParts { name, format, bytes })
    }

    fn game_id(raw: &str) -> Result<Uuid, V2Error> {
        Uuid::parse_str(raw)
            .map_err(|_| V2Error::invalid_argument("gameId must be a UUID."))
    }

    fn version(raw: &str) -> Result<i32, V2Error> {
        raw.parse::<i32>()
            .map_err(|_| V2Error::invalid_argument("version must be a number."))
    }
}

/// One parsed upload: the stored display name, the gated format and the bytes.
struct UploadParts {
    name: String,
    format: engine_config::MapFormat,
    bytes: Vec<u8>,
}
