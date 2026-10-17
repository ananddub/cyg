//! Auto-registered bindings for `/v2/console/audit/log` and
//! `/v2/console/acl/template`.

use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::{Body, Params};
use crate::current_state;
use crate::dto::console::audit::{AclTemplateRequest, AuditLogQuery};
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
    async fn list(&self, _auth: ConsoleAuthed, Params(query): Params<AuditLogQuery>) -> Response {
        let state = current_state();
        self.service
            .list_audit_logs(&state, query)
            .await
            .into_response()
    }

    /// The usernames the audit view filters by, grouped from the trail itself.
    #[get("/log/users")]
    async fn list_users(&self, _auth: ConsoleAuthed) -> Response {
        let state = current_state();
        self.service.list_audit_log_users(&state).await.into_response()
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

    #[post("/template")]
    async fn create(
        &self,
        auth: ConsoleAuthed,
        Body(body): Body<AclTemplateRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .create_acl_template(&state, &auth, body)
            .await
            .into_response()
    }

    #[put("/template/{id}")]
    async fn update(
        &self,
        auth: ConsoleAuthed,
        Path(id): Path<String>,
        Body(body): Body<AclTemplateRequest>,
    ) -> Response {
        let state = current_state();
        self.service
            .update_acl_template(&state, &auth, &id, body)
            .await
            .into_response()
    }

    #[delete("/template/{id}")]
    async fn delete(&self, auth: ConsoleAuthed, Path(id): Path<String>) -> Response {
        let state = current_state();
        self.service
            .delete_acl_template(&state, &auth, &id)
            .await
            .into_response()
    }
}
