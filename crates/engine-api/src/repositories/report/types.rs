use super::*;

pub struct CreateReportParams {
    pub id: Uuid,
    pub reporter_id: Uuid,
    pub reported_id: Uuid,
    pub reason: String,
    pub details: String,
    pub path: Option<String>,
}

pub struct ListReportsPagedParams {
    pub create_time: DateTime<Utc>,
    pub id_start: Uuid,
    pub limit: i64,
}

pub struct ListReportsByStatusPagedParams {
    pub status: String,
    pub create_time: DateTime<Utc>,
    pub id_start: Uuid,
    pub limit: i64,
}

pub struct CreateInfractionParams {
    pub id: Uuid,
    pub user_id: Uuid,
    pub kind: String,
    pub note: String,
    pub expires_at: DateTime<Utc>,
    pub issued_by: Option<Uuid>,
}
