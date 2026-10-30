use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("content_report", schemaless)]
#[index(name = "content_report_reported_id", fields = "reported_id")]
#[index(name = "content_report_status", fields = "status")]
pub struct ContentReport {
    #[field(thing)]
    pub id: Uuid,
    pub reporter_id: Uuid,
    pub reported_id: Uuid,
    pub reason: String,
    pub details: String,
    pub path: Option<String>,
    pub status: String,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("user_infraction", schemaless)]
#[index(name = "user_infraction_user_id", fields = "user_id")]
pub struct UserInfraction {
    #[field(thing)]
    pub id: Uuid,
    pub user_id: Uuid,
    pub kind: String,
    pub note: String,
    #[serde(with = "time::iso")]
    pub expires_at: DateTime<Utc>,
    pub issued_by: Option<Uuid>,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
}
