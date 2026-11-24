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

type SettingRowList = Vec<crate::dto::console::setting::SettingRow>;
#[controller("/v1/console/setting")]
impl ConsoleSettingController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("", response_body = SettingRowList)]
    async fn list(&self, _auth: ConsoleAuthed) -> Response {
        let state = current_state();
        self.service.list_settings().await.into_response()
    }

    #[put("", response_body = crate::dto::console::setting::SettingRow)]
    async fn update(
        &self,
        auth: ConsoleAuthed,
        Body(body): Body<UpdateSettingRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .update_setting(&auth, body)
            .await
            .into_response()
    }

    #[delete("/{name}", response_body = crate::dto::Empty)]
    async fn delete(&self, auth: ConsoleAuthed, Path(name): Path<String>) -> Response {
        let state = current_state();
        self.service
            .delete_setting(&auth, &name)
            .await
            .into_response()
    }
}
