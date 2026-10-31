use super::*;

impl ConsoleMapsController {
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
        self.service
            .upload_map(&state, game_id, upload.name, upload.format, upload.bytes)
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

    pub(super) fn game_id(raw: &str) -> Result<Uuid, V2Error> {
        Uuid::parse_str(raw)
            .map_err(|_| V2Error::invalid_argument("gameId must be a UUID."))
    }

    pub(super) fn version(raw: &str) -> Result<i32, V2Error> {
        raw.parse::<i32>()
            .map_err(|_| V2Error::invalid_argument("version must be a number."))
    }
}
pub(super) struct UploadParts {
    name: String,
    format: engine_config::MapFormat,
    bytes: Vec<u8>,
}
