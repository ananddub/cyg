//! Auto-registered bindings for `/v2/console/audit/log` and
//! `/v2/console/acl/template`.

use std::sync::Arc;

use auto_route::controller;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::Params;
use crate::current_state;
use crate::dto::console::Page;
use crate::services::console::ConsoleService;

struct ConsoleAuditController {
    service: Arc<ConsoleService>,
}

#[controller("/v2/console/audit")]
impl ConsoleAuditController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("/log")]
    async fn list(&self, _auth: ConsoleAuthed, Params(page): Params<Page>) -> Response {
        let state = current_state();
        self.service.list_audit_logs(&state, page).await.into_response()
    }
}

struct ConsoleAclController {
    service: Arc<ConsoleService>,
}

#[controller("/v2/console/acl")]
impl ConsoleAclController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("/template")]
    async fn list(&self, _auth: ConsoleAuthed) -> Response {
        let state = current_state();
        self.service
            .list_acl_templates(&state)
            .await
            .into_response()
    }
}
