use axum::Json;

use crate::dto::console::logs::LogLineResponse;
use crate::infra::logfeed;
use engine_dashboard::models::log::LogLine;

const MAX_LIMIT: usize = 500;

impl super::ConsoleService {
    pub async fn recent_logs(&self, limit: Option<usize>) -> Json<Vec<LogLineResponse>> {
        Json(
            logfeed::recent(limit.unwrap_or(200).min(MAX_LIMIT))
                .into_iter()
                .map(LogLineResponse::from)
                .collect(),
        )
    }
}

impl From<LogLine> for LogLineResponse {
    fn from(line: LogLine) -> Self {
        Self {
            at_ms: line.at_ms,
            level: line.level,
            target: line.target,
            message: line.message,
        }
    }
}
