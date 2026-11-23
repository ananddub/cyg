use std::sync::Arc;

use auto_route::controller;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use crate::app::middleware::ConsoleAuthed;
use crate::app::params::Params;
use crate::services::console::ConsoleService;

#[derive(Debug, Default, Deserialize)]
struct LogsQuery {
    #[serde(default)]
    limit: Option<usize>,
}

struct ConsoleLogsController {
    service: Arc<ConsoleService>,
}

type LogLineResponseList = Vec<crate::dto::console::logs::LogLineResponse>;
#[controller("/v1/console/logs")]
impl ConsoleLogsController {
    fn new(service: Arc<ConsoleService>) -> Self {
        Self { service }
    }

    #[get("/recent", response_body = LogLineResponseList)]
    async fn recent(&self, _auth: ConsoleAuthed, Params(query): Params<LogsQuery>) -> Response {
        self.service.recent_logs(query.limit).await.into_response()
    }
}
