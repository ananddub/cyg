use std::sync::Arc;

use auto_route::controller;
use axum::Json;
use axum::extract::Path;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::{Body, Params};
use crate::current_state;
use crate::dto::console::audit::{AclTemplateRequest, AuditLogQuery};
use crate::services::console::ConsoleService;
use crate::utils::precondition::{self, ETAG};

struct ConsoleAuditController {
    service: Arc<ConsoleService>,
}

#[controller("/v1/console/audit")]
impl ConsoleAuditController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("/log")]
    async fn list(&self, _auth: ConsoleAuthed, Params(query): Params<AuditLogQuery>) -> Response {
        let state = current_state();
        self.service.list_audit_logs(query).await.into_response()
    }

    #[get("/log/users")]
    async fn list_users(&self, _auth: ConsoleAuthed) -> Response {
        let state = current_state();
        self.service.list_audit_log_users().await.into_response()
    }
}

struct ConsoleAclController {
    service: Arc<ConsoleService>,
}

#[controller("/v1/console/acl")]
impl ConsoleAclController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("/template")]
    async fn list(&self, _auth: ConsoleAuthed) -> Response {
        let state = current_state();
        self.service.list_acl_templates().await.into_response()
    }

    #[post("/template")]
    async fn create(&self, auth: ConsoleAuthed, Body(body): Body<AclTemplateRequest>) -> Response {
        let state = current_state();
        self.service
            .create_acl_template(&auth, body)
            .await
            .into_response()
    }

    #[put("/template/{id}")]
    async fn update(
        &self,
        auth: ConsoleAuthed,
        Path(id): Path<String>,
        headers: HeaderMap,
        Body(body): Body<AclTemplateRequest>,
    ) -> Response {
        let state = current_state();
        let expected = match precondition::expected_version(&headers) {
            Ok(expected) => expected,
            Err(err) => return err.into_response(),
        };
        match self
            .service
            .update_acl_template(&auth, &id, body, expected)
            .await
        {
            Ok(Json(row)) => {
                let version = row.version;
                let mut response = Json(row).into_response();
                if let Ok(value) = precondition::tag(version).parse() {
                    response.headers_mut().insert(ETAG, value);
                }
                response
            }
            Err(err) => err.into_response(),
        }
    }

    #[delete("/template/{id}")]
    async fn delete(&self, auth: ConsoleAuthed, Path(id): Path<String>) -> Response {
        let state = current_state();
        self.service
            .delete_acl_template(&auth, &id)
            .await
            .into_response()
    }
}
