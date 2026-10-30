use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::repositories::models::{DeliveryAttempt, NotificationPreference};

/// One switch on the account's notification settings. `kind` is `*` for the
/// whole channel.
#[derive(Debug, Default, Deserialize)]
pub struct PreferenceRequest {
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct PreferenceRow {
    pub channel: String,
    pub kind: String,
    pub enabled: bool,
    pub update_time: DateTime<Utc>,
}

impl From<NotificationPreference> for PreferenceRow {
    fn from(row: NotificationPreference) -> Self {
        Self {
            channel: row.channel,
            kind: row.kind,
            enabled: row.enabled,
            update_time: row.update_time,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PreferenceListResponse {
    pub success: bool,
    pub preferences: Vec<PreferenceRow>,
}

#[derive(Debug, Serialize)]
pub struct PreferenceResponse {
    pub success: bool,
    pub preference: PreferenceRow,
}

/// What an operator asks for when they want to know whether the delivery path
/// works: one provider, and the address or token to try it with.
#[derive(Debug, Default, Deserialize)]
pub struct TestRequest {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub recipient: String,
}

#[derive(Debug, Serialize)]
pub struct AttemptRow {
    pub id: String,
    pub notification_id: String,
    pub channel: String,
    pub provider: String,
    pub status: String,
    pub attempts: i64,
    pub error: String,
    pub next_retry_at: DateTime<Utc>,
}

impl From<&DeliveryAttempt> for AttemptRow {
    fn from(row: &DeliveryAttempt) -> Self {
        Self {
            id: row.id.to_string(),
            notification_id: row.notification_id.to_string(),
            channel: row.channel.clone(),
            provider: row.provider.clone(),
            status: row.status.clone(),
            attempts: row.attempts,
            error: row.error.clone(),
            next_retry_at: row.next_retry_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AttemptResponse {
    pub success: bool,
    pub attempt: AttemptRow,
}

/// The query of the unsubscribe link a mail footer carries.
#[derive(Debug, Deserialize)]
pub struct UnsubscribeQuery {
    #[serde(default)]
    pub token: String,
}

#[derive(Debug, Serialize)]
pub struct UnsubscribeResponse {
    pub success: bool,
    pub channel: String,
    pub message: String,
}
