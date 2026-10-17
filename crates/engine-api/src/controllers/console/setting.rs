//! Auto-registered bindings for `/v2/console/setting` — the server's key/value
//! settings, read and written.

use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::Body;
use crate::current_state;
use crate::dto::console::setting::UpdateSettingRequest;
use crate::services::console::ConsoleService;

struct ConsoleSettingController {
    service: Arc<ConsoleService>,
}

#[controller("/v2/console/setting")]
impl ConsoleSettingController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn list(&self, _auth: ConsoleAuthed) -> Response {
        let state = current_state();
        self.service.list_settings(&state).await.into_response()
    }

    #[put("")]
    async fn update(
        &self,
        auth: ConsoleAuthed,
        Body(body): Body<UpdateSettingRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .update_setting(&state, &auth, body)
            .await
            .into_response()
    }

    #[delete("/{name}")]
    async fn delete(&self, auth: ConsoleAuthed, Path(name): Path<String>) -> Response {
        let state = current_state();
        self.service
            .delete_setting(&state, &auth, &name)
            .await
            .into_response()
    }
}
