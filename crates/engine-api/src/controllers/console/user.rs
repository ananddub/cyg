//! Auto-registered bindings for `/v2/console/user*` — the console operators an admin
//! manages.

use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::{Body, Params};
use crate::current_state;
use crate::dto::console::Page;
use crate::dto::console::user::{
    CreateConsoleUserRequest, SetConsolePasswordRequest, UpdateConsoleUserRequest,
};
use crate::services::console::ConsoleService;

struct ConsoleUserController {
    service: Arc<ConsoleService>,
}

#[controller("/v2/console/user")]
impl ConsoleUserController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("")]
    async fn list(&self, _auth: ConsoleAuthed, Params(page): Params<Page>) -> Response {
        let state = current_state();
        self.service
            .list_console_users(&state, page)
            .await
            .into_response()
    }

    #[post("")]
    async fn create(
        &self,
        auth: ConsoleAuthed,
        Body(body): Body<CreateConsoleUserRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .create_console_user(&state, &auth, body)
            .await
            .into_response()
    }

    #[put("/{id}")]
    async fn update(
        &self,
        auth: ConsoleAuthed,
        Path(id): Path<String>,
        Body(body): Body<UpdateConsoleUserRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .update_console_user(&state, &auth, &id, body)
            .await
            .into_response()
    }

    #[delete("/{id}")]
    async fn delete(&self, auth: ConsoleAuthed, Path(id): Path<String>) -> Response {
        let state = current_state();
        self.service
            .delete_console_user(&state, &auth, &id)
            .await
            .into_response()
    }

    #[post("/{id}/password")]
    async fn password(
        &self,
        auth: ConsoleAuthed,
        Path(id): Path<String>,
        Body(body): Body<SetConsolePasswordRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .set_console_user_password(&state, &auth, &id, body)
            .await
            .into_response()
    }

    #[post("/{id}/disable")]
    async fn disable(&self, auth: ConsoleAuthed, Path(id): Path<String>) -> Response {
        let state = current_state();
        self.service
            .set_console_user_disabled(&state, &auth, &id, true)
            .await
            .into_response()
    }

    #[post("/{id}/enable")]
    async fn enable(&self, auth: ConsoleAuthed, Path(id): Path<String>) -> Response {
        let state = current_state();
        self.service
            .set_console_user_disabled(&state, &auth, &id, false)
            .await
            .into_response()
    }

    #[delete("/{id}/mfa")]
    async fn clear_mfa(&self, auth: ConsoleAuthed, Path(id): Path<String>) -> Response {
        let state = current_state();
        self.service
            .clear_console_user_mfa(&state, &auth, &id)
            .await
            .into_response()
    }
}
