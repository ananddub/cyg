use super::*;

pub(super) const MAX_UPLOAD_BYTES: usize = 16 * 1024 * 1024;
pub(super) const MAX_FILENAME_LEN: usize = 255;
pub(super) struct ConsoleWasmController {
    pub(super) service: Arc<WasmService>,
}
type ModuleDtoList = Vec<crate::dto::console::wasm::ModuleDto>;
#[controller("/v1/console/games/{gameId}/wasm")]
impl ConsoleWasmController {
    fn new(service: Arc<WasmService>) -> Self {
        Self { service }
    }

    #[post("/upload", response_body = crate::dto::console::wasm::UploadResponse)]
    async fn upload(
        &self,
        auth: ConsoleAuthed,
        Path(game_id): Path<String>,
        request: Request,
    ) -> Response {
        self.store_upload(auth, game_id, request).await
    }

    #[post("", response_body = crate::dto::console::wasm::UploadResponse)]
    async fn upload_at_root(
        &self,
        auth: ConsoleAuthed,
        Path(game_id): Path<String>,
        request: Request,
    ) -> Response {
        self.store_upload(auth, game_id, request).await
    }

    #[get("", response_body = ModuleDtoList)]
    async fn list(&self, _auth: ConsoleAuthed, Path(game_id): Path<String>) -> Response {
        let state = current_state();
        let game_id = match Self::game_id(&game_id) {
            Ok(id) => id,
            Err(e) => return e.into_response(),
        };
        self.service
            .list_modules(game_id)
            .await
            .map(Json)
            .into_response()
    }

    #[post("/activate", response_body = crate::dto::console::wasm::ModuleDto)]
    async fn activate(
        &self,
        auth: ConsoleAuthed,
        Path(game_id): Path<String>,
        Body(body): Body<ActivateRequest>,
    ) -> Response {
        self.switch(auth, game_id, body.version, ReloadKind::Upload)
            .await
    }

    #[post("/rollback", response_body = crate::dto::console::wasm::ModuleDto)]
    async fn rollback(
        &self,
        auth: ConsoleAuthed,
        Path(game_id): Path<String>,
        Body(body): Body<ActivateRequest>,
    ) -> Response {
        self.switch(auth, game_id, body.version, ReloadKind::Rollback)
            .await
    }

    #[delete("/{version}", response_body = crate::dto::Empty)]
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
            .delete_module(game_id, version)
            .await
            .into_response()
    }
}
pub(super) struct UploadParts {
    pub(super) filename: String,
    pub(super) binary: Vec<u8>,
    pub(super) source_code: Option<String>,
}
