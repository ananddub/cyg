use super::*;

impl ConsoleWasmController {
    pub(super) async fn store_upload(
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

    pub(super) async fn switch(
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

    pub(super) fn game_id(raw: &str) -> Result<Uuid, V2Error> {
        Uuid::parse_str(raw).map_err(|_| V2Error::invalid_argument("Invalid game id."))
    }

    pub(super) fn version(raw: &str) -> Result<i32, V2Error> {
        raw.parse::<i32>()
            .map_err(|_| V2Error::invalid_argument("Invalid module version."))
    }
}
