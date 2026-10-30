use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::{Authed, ConsoleAuthed};
use crate::app::params::{Body, Params};
use crate::current_state;
use crate::dto::report::{CreateReportRequest, ListQuery, StatusRequest};

use crate::services::report::ReportService;

struct ReportController {
    service: Arc<ReportService>,
}

#[controller("/v2/reports")]
impl ReportController {
    fn new(service: Arc<ReportService>) -> Self {
        Self { service }
    }

    #[post("")]
    async fn create_report(&self, auth: Authed, Body(req): Body<CreateReportRequest>) -> Response {
        let state = current_state();
        self.service.create(&state, &auth, req).await.into_response()
    }

    #[get("")]
    async fn list_reports(&self, auth: ConsoleAuthed, Params(req): Params<ListQuery>) -> Response {
        let state = current_state();
        self.service.list(&state, &auth, req).await.into_response()
    }

    #[put("/{id}/status")]
    async fn set_status(
        &self,
        auth: ConsoleAuthed,
        Path(id): Path<String>,
        Body(req): Body<StatusRequest>,
    ) -> Response {
        let state = current_state();
        self.service.set_status(&state, &auth, &id, req).await.into_response()
    }
}
