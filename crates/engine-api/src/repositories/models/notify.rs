use super::*;

/// What one account has opted out of. There is no row for a notification type
/// nobody has spoken about: the absence of a row is *yes, send it*, so turning
/// the whole surface on never needs a write per account. `kind` is `*` for the
/// whole channel and a specific kind outranks the wildcard.
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("notification_preference", schemaless)]
#[index(
    name = "notification_preference_user_uniq",
    fields = "user_id, channel, kind",
    unique
)]
pub struct NotificationPreference {
    #[field(thing)]
    pub id: String,
    pub user_id: Uuid,
    /// `push` or `email`.
    pub channel: String,
    /// One kind's name, or `*` for everything on the channel.
    pub kind: String,
    pub enabled: bool,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
}

/// One message owed to one vendor. Written by the request that made the fact
/// true and sent later by the outbox worker, never inline: a relay that takes
/// ten seconds to answer must not hold up a player.
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("delivery_attempt", schemaless)]
#[index(name = "delivery_attempt_due", fields = "status, next_retry_at")]
#[index(
    name = "delivery_attempt_target_uniq",
    fields = "notification_id, channel, provider",
    unique
)]
pub struct DeliveryAttempt {
    #[field(thing)]
    pub id: Uuid,
    pub user_id: Uuid,
    /// The in-app row this message is the outside world's copy of. A replayed
    /// fact cannot owe the same vendor twice, which is what the unique index on
    /// it is for.
    pub notification_id: Uuid,
    pub channel: String,
    /// `apns`, `fcm` or `smtp` — the sender the worker hands this row to.
    pub provider: String,
    /// The device token or address, as it was when the fact happened.
    pub recipient: String,
    pub subject: String,
    pub text: String,
    pub status: String,
    pub attempts: i64,
    pub error: String,
    #[serde(with = "time::iso")]
    pub next_retry_at: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
}
