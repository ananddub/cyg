use serde::{Deserialize, Serialize};

use crate::repositories::models::ContentReport;
use crate::services::social::{ts, uid};

pub const REASONS: [&str; 5] = [
    "harassment",
    "cheating",
    "exploit",
    "inappropriate_content",
    "other",
];
pub const STATUSES: [&str; 3] = ["open", "actioned", "dismissed"];

#[derive(Debug, Default, Deserialize)]
pub struct CreateReportRequest {
    #[serde(default, rename = "reportedId", alias = "reported_id")]
    pub reported_id: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub details: String,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default, rename = "cacheableCursor", alias = "cacheable_cursor")]
    pub cacheable_cursor: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct StatusRequest {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default, rename = "durationSecs", alias = "duration_secs")]
    pub duration_secs: Option<i64>,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportResponse {
    pub id: String,
    pub reporter_id: String,
    pub reported_id: String,
    pub reason: String,
    pub details: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub status: String,
    pub create_time: String,
    pub update_time: String,
}

impl From<&ContentReport> for ReportResponse {
    fn from(r: &ContentReport) -> Self {
        ReportResponse {
            id: uid(&r.id),
            reporter_id: uid(&r.reporter_id),
            reported_id: uid(&r.reported_id),
            reason: r.reason.clone(),
            details: r.details.clone(),
            path: r.path.clone(),
            status: r.status.clone(),
            create_time: ts(r.create_time),
            update_time: ts(r.update_time),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ReportListResponse {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reports: Vec<ReportResponse>,
    pub cacheable_cursor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfractionResponse {
    pub id: String,
    pub user_id: String,
    pub kind: String,
    pub note: String,
    pub expires_at: String,
    pub create_time: String,
}

impl From<&crate::repositories::models::UserInfraction> for InfractionResponse {
    fn from(i: &crate::repositories::models::UserInfraction) -> Self {
        InfractionResponse {
            id: uid(&i.id),
            user_id: uid(&i.user_id),
            kind: i.kind.clone(),
            note: i.note.clone(),
            expires_at: ts(i.expires_at),
            create_time: ts(i.create_time),
        }
    }
}
