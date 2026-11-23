use poem_openapi::Object;
use serde::Serialize;

/// One line of the server's in-memory log ring.
#[derive(Debug, Clone, Serialize, Object)]
pub struct LogLineResponse {
    pub at_ms: i64,
    pub level: String,
    pub target: String,
    pub message: String,
}
