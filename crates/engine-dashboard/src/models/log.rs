use serde::Serialize;

/// One mirrored log event. `atMs` is wall-clock millis; the live feed may
/// drop lines under load, which the client sees as a gap in `atMs`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub at_ms: i64,
    pub level: String,
    pub target: String,
    pub message: String,
}
