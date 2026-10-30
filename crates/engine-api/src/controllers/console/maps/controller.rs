use super::*;

pub(super) const MAX_UPLOAD_BYTES: usize = 32 * 1024 * 1024;
pub(super) const MAX_NAME_LEN: usize = 255;
pub(super) const EXT_FORMATS: &[(&str, &str)] = &[
    ("oxcl", "oxcl"),
    ("json", "map2d"),
    ("nav2d", "navmesh"),
    ("mesh", "navmesh"),
];
pub(super) struct ConsoleMapsController {
    pub(super) service: Arc<MapService>,
}
#[controller("/v2/console/games/{gameId}/maps")]
impl ConsoleMapsController {
    fn new(service: Arc<MapService>) -> Self {
        Self { service }
    }

    #[post("/upload")]
    async fn upload(
        &self,
        auth: ConsoleAuthed,
        Path(game_id): Path<String>,
        request: Request,
    ) -> Response {
        self.store_upload(auth, game_id, request).await
    }

    #[post("")]
    async fn upload_at_root(
        &self,
        auth: ConsoleAuthed,
        Path(game_id): Path<String>,
        request: Request,
    ) -> Response {
        self.store_upload(auth, game_id, request).await
    }

    #[get("")]
    async fn list(&self, _auth: ConsoleAuthed, Path(game_id): Path<String>) -> Response {
        let state = current_state();
        let game_id = match Self::game_id(&game_id) {
            Ok(id) => id,
            Err(e) => return e.into_response(),
        };
        self.service
            .list_maps(&state, game_id)
            .await
            .map(Json)
            .into_response()
    }

    #[get("/{version}/preview")]
    async fn preview(
        &self,
        _auth: ConsoleAuthed,
        Path((game_id, version)): Path<(String, String)>,
    ) -> Response {
        let state = current_state();
        let parsed =
            Self::game_id(&game_id).and_then(|id| Self::version(&version).map(|v| (id, v)));
        let (game_id, version) = match parsed {
            Ok(pair) => pair,
            Err(e) => return e.into_response(),
        };
        self.service
            .preview_map(&state, game_id, version)
            .await
            .into_response()
    }

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
