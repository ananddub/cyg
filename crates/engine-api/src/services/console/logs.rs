use axum::Json;

use crate::infra::logfeed;
use engine_dashboard::models::log::LogLine;

const MAX_LIMIT: usize = 500;

impl super::ConsoleService {
    pub async fn recent_logs(&self, limit: Option<usize>) -> Json<Vec<LogLine>> {
        Json(logfeed::recent(limit.unwrap_or(200).min(MAX_LIMIT)))
    }
}
