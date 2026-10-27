//! Auto-registered bindings for `/v2/console/games/{gameId}/wasm*` — the
//! dynamic room module of one game: upload (multipart, ABI-gated), list,
//! activate/rollback, and delete of a stored version.
//!
//! Transport only, like every console controller: parse the multipart into
//! bytes, hand the rest to [`WasmService`], and return whatever it shapes.

use std::sync::Arc;

use auto_route::controller;
use axum::Json;
use axum::extract::{DefaultBodyLimit, FromRequest, Multipart, Path, Request};
use axum::response::{IntoResponse, Response};
use engine_bus::ReloadKind;
use uuid::Uuid;

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::Body;
use crate::current_state;
use crate::dto::console::wasm::ActivateRequest;
use crate::error::V2Error;
use crate::services::wasm::WasmService;

/// One upload request — module, source and multipart framing — capped at the
/// host's own 16 MiB state bound (`engine_wasm::abi::MAX_SAVE_STATE_SIZE`):
/// the console must not store a module the runtime would refuse to load.
const MAX_UPLOAD_BYTES: usize = 16 * 1024 * 1024;

/// What `room_modules.filename` can hold; longer names are refused, not
/// silently truncated, so the listing matches what the upload sent.
const MAX_FILENAME_LEN: usize = 255;

struct ConsoleWasmController {
    service: Arc<WasmService>,
}

#[controller("/v2/console/games/{gameId}/wasm")]
impl ConsoleWasmController {
    fn new(service: Arc<WasmService>) -> Self {
        Self { service }
    }

    /// `POST …/upload` — multipart with a required `file` field (the `.wasm`)
    /// and an optional `source_code` text field. The ABI gate runs before the
    /// database sees anything: a module missing host-required exports is a
    /// 400 naming them, not a stored dead version.
    #[post("/upload")]
    async fn upload(
        &self,
        auth: ConsoleAuthed,
        Path(game_id): Path<String>,
        request: Request,
    ) -> Response {
        self.store_upload(auth, game_id, request).await
    }

    /// `POST …/wasm` — the same upload at the collection path: a POST to a
    /// game's module collection creates a module, however the client spells
    /// it. One shared implementation, so the two paths can never diverge.
    #[post("")]
    async fn upload_at_root(
        &self,
        auth: ConsoleAuthed,
        Path(game_id): Path<String>,
        request: Request,
    ) -> Response {
        self.store_upload(auth, game_id, request).await
    }

    /// `GET …/wasm` — every stored version, newest first, binaries excluded.
    #[get("")]
    async fn list(&self, _auth: ConsoleAuthed, Path(game_id): Path<String>) -> Response {
        let state = current_state();
        let game_id = match Self::game_id(&game_id) {
            Ok(id) => id,
            Err(e) => return e.into_response(),
        };
        self.service
            .list_modules(&state, game_id)
            .await
            .map(Json)
            .into_response()
    }

    /// `POST …/activate` — make a stored version active and publish the
    /// hot-reload. `version` is just a number here: activating the newest is
    /// an upgrade and activating an older one is a rollback, same flip.
    #[post("/activate")]
    async fn activate(
        &self,
        auth: ConsoleAuthed,
        Path(game_id): Path<String>,
        Body(body): Body<ActivateRequest>,
    ) -> Response {
        self.switch(auth, game_id, body.version, ReloadKind::Upload)
            .await
    }

    /// `POST …/rollback` — the same flip with the reason an operator reaches
    /// for when the newest module is the broken one; the bus message records
    /// it so a sim's log says which way the swap went.
    #[post("/rollback")]
    async fn rollback(
        &self,
        auth: ConsoleAuthed,
        Path(game_id): Path<String>,
        Body(body): Body<ActivateRequest>,
    ) -> Response {
        self.switch(auth, game_id, body.version, ReloadKind::Rollback)
            .await
    }

    /// `DELETE …/{version}` — drop a stored version; the active one is refused
    /// so a live sim's next reload can never point at deleted bytes.
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
            .delete_module(&state, game_id, version)
            .await
            .into_response()
    }
}

/// One parsed upload: the stored filename, the validated bytes, and the
/// optional source listing that came alongside them.
struct UploadParts {
    filename: String,
    binary: Vec<u8>,
    source_code: Option<String>,
}

impl ConsoleWasmController {
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
        match self.service
            .upload_module(
                &state,
                game_id,
                upload.filename,
                upload.binary.clone(),
                upload.source_code,
            )
            .await
        {
            Ok(resp) => Json(resp).into_response(),
            Err(e) => {
                let report = WasmService::validate_binary(&upload.binary);
                let status = e.kind.status();
                let body = serde_json::json!({
                    "error": {
                        "code": e.kind.code(),
                        "message": e.message,
                        "details": Vec::<String>::new(),
                    },
                    "abi": report
                });
                (status, Json(body)).into_response()
            }
        }
    }

    async fn switch(
        &self,
        auth: ConsoleAuthed,
        game_id: String,
        version: i32,
        kind: ReloadKind,
    ) -> Response {
        let state = current_state();
        let game_id = match auth.require_write().and_then(|()| Self::game_id(&game_id)) {
            Ok(id) => id,
            Err(e) => return e.into_response(),
        };
        self.service
            .activate_module(&state, game_id, version, kind)
            .await
            .map(Json)
            .into_response()
    }

    /// Drain the multipart body into [`UploadParts`]. The router-wide 2 MiB
    /// request cap is lifted for this one request by inserting the
    /// `DefaultBodyLimit` extension its `Multipart` extractor reads — no
    /// global layer, no other route sees a bigger limit.
    async fn read_upload(mut request: Request) -> Result<UploadParts, V2Error> {
        request
            .extensions_mut()
            .insert(DefaultBodyLimit::max(MAX_UPLOAD_BYTES));
        let mut multipart = Multipart::from_request(request, &())
            .await
            .map_err(|e| V2Error::invalid_argument(format!("Invalid multipart upload: {e}")))?;

        let mut filename: Option<String> = None;
        let mut binary: Option<Vec<u8>> = None;
        let mut source_code: Option<String> = None;
        while let Some(field) = multipart
            .next_field()
            .await
            .map_err(|e| V2Error::invalid_argument(format!("Multipart read failed: {e}")))?
        {
            match field.name().unwrap_or_default() {
                "file" => {
                    // `file_name()` borrows the field and `bytes()` consumes
                    // it, so the declared name is captured first.
                    let name = field.file_name().unwrap_or("room_logic.wasm").to_owned();
                    if name.len() > MAX_FILENAME_LEN {
                        return Err(V2Error::invalid_argument(format!(
                            "Filename is longer than {MAX_FILENAME_LEN} characters."
                        )));
                    }
                    binary = Some(
                        field
                            .bytes()
                            .await
                            .map_err(|e| {
                                V2Error::invalid_argument(format!("Failed to read file: {e}"))
                            })?
                            .to_vec(),
                    );
                    filename = Some(name);
                }
                "source_code" => {
                    let bytes = field.bytes().await.map_err(|e| {
                        V2Error::invalid_argument(format!("Failed to read source: {e}"))
                    })?;
                    source_code = Some(String::from_utf8_lossy(&bytes).into_owned());
                }
                // Unknown fields are skipped, not rejected: a client that
                // sends extra parts should still get its module stored.
                _ => {}
            }
        }
        let binary = binary
            .filter(|b| !b.is_empty())
            .ok_or_else(|| V2Error::invalid_argument("Upload needs a non-empty `file` field."))?;
        Ok(UploadParts {
            filename: filename.unwrap_or_else(|| "room_logic.wasm".to_owned()),
            binary,
            source_code,
        })
    }

    fn game_id(raw: &str) -> Result<Uuid, V2Error> {
        Uuid::parse_str(raw).map_err(|_| V2Error::invalid_argument("Invalid game id."))
    }

    fn version(raw: &str) -> Result<i32, V2Error> {
        raw.parse::<i32>()
            .map_err(|_| V2Error::invalid_argument("Invalid module version."))
    }
}
