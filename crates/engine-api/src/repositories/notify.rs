#![allow(unused_imports, dead_code)]

use super::db::{Queries, WriteResult, affected, params, ts};
use super::models::{DeliveryAttempt, NotificationPreference};
use crate::db::{DbError, Key, time};
use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

/// The states one message moves through. Only `queued` is eligible to be sent.
pub const QUEUED: &str = "queued";
pub const SENDING: &str = "sending";
pub const DELIVERED: &str = "delivered";
pub const DEAD: &str = "dead";

/// The one row a channel and a kind share per account, keyed so a write cannot
/// race itself into a second opinion about the same choice.
pub fn preference_id(user_id: Uuid, channel: &str, kind: &str) -> String {
    format!("{user_id}:{channel}:{kind}")
}

impl Queries {
    pub async fn list_preferences(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<NotificationPreference>, DbError> {
        self.all(
            "SELECT * FROM notification_preference WHERE user_id = $user \
             ORDER BY channel ASC, kind ASC",
            params!(user = user_id.to_string()),
        )
        .await
    }

    pub async fn preference(
        &self,
        user_id: Uuid,
        channel: &str,
        kind: &str,
    ) -> Result<Option<NotificationPreference>, DbError> {
        self.opt(
            "SELECT * FROM type::record('notification_preference', $id) LIMIT 1",
            params!(id = preference_id(user_id, channel, kind)),
        )
        .await
    }

    /// Writes a choice over itself. There is no create-only path here: opting out
    /// and opting back in are the same statement about the same channel.
    pub async fn put_preference(&self, row: &NotificationPreference) -> Result<(), DbError> {
        let mut data = serde_json::to_value(row)?;
        if let Value::Object(map) = &mut data {
            map.remove("id");
        }
        self.exec(
            "UPSERT type::record('notification_preference', $id) CONTENT $data",
            params!(id = row.id.clone(), data = data),
        )
        .await
    }

    /// `false` means this notification was already owed to this vendor.
    pub async fn queue_attempt(&self, row: &DeliveryAttempt) -> Result<bool, DbError> {
        match self
            .create("delivery_attempt", Key::Uuid(row.id), row)
            .await
        {
            Ok(_) => Ok(true),
            Err(err) if err.is_unique() => Ok(false),
            Err(err) => Err(err),
        }
    }

    pub async fn attempt(&self, id: Uuid) -> Result<Option<DeliveryAttempt>, DbError> {
        self.opt(
            "SELECT * FROM type::record('delivery_attempt', <uuid>$id) LIMIT 1",
            params!(id = id.to_string()),
        )
        .await
    }

    pub async fn list_attempts(
        &self,
        user_id: Uuid,
        limit: i64,
    ) -> Result<Vec<DeliveryAttempt>, DbError> {
        self.all(
            "SELECT * FROM delivery_attempt WHERE user_id = $user \
             ORDER BY create_time DESC, id ASC LIMIT $limit",
            params!(user = user_id.to_string(), limit = limit),
        )
        .await
    }

    pub async fn due_attempts(&self, limit: i64) -> Result<Vec<DeliveryAttempt>, DbError> {
        self.all(
            "SELECT * FROM delivery_attempt WHERE status = 'queued' \
               AND next_retry_at <= $now \
             ORDER BY next_retry_at ASC, id ASC LIMIT $limit",
            params!(now = ts(Utc::now()), limit = limit),
        )
        .await
    }

    /// Takes a queued row for this worker alone; a second worker reading the same
    /// due list gets nothing.
    pub async fn claim_attempt(&self, id: Uuid) -> Result<Option<DeliveryAttempt>, DbError> {
        self.opt(
            "UPDATE type::record('delivery_attempt', <uuid>$id) \
             SET status = 'sending', update_time = $now WHERE status = 'queued' RETURN AFTER",
            params!(id = id.to_string(), now = ts(Utc::now())),
        )
        .await
    }

    /// Writes the outcome of one attempt, guarded on the row still being this
    /// worker's so a reaper that handed it to somebody else is not overwritten.
    pub async fn settle_attempt(
        &self,
        id: Uuid,
        status: &str,
        attempts: i64,
        error: &str,
        next_retry_at: DateTime<Utc>,
    ) -> Result<(), DbError> {
        self.exec(
            "UPDATE type::record('delivery_attempt', <uuid>$id) \
             SET status = $status, attempts = $attempts, error = $error, \
                 next_retry_at = $next, update_time = $now \
             WHERE status = 'sending'",
            params!(
                id = id.to_string(),
                status = status,
                attempts = attempts,
                error = error,
                next = ts(next_retry_at),
                now = ts(Utc::now()),
            ),
        )
        .await
    }

    /// Keeps a row owed without counting the try: this is the account's budget
    /// being full, not the vendor saying no, and nothing was spent or sent.
    pub async fn defer_attempt(&self, id: Uuid, until: DateTime<Utc>) -> Result<(), DbError> {
        self.exec(
            "UPDATE type::record('delivery_attempt', <uuid>$id) \
             SET status = 'queued', next_retry_at = $until, update_time = $now \
             WHERE status = 'sending'",
            params!(id = id.to_string(), until = ts(until), now = ts(Utc::now())),
        )
        .await
    }

    /// Drops an owed message without ever sending it, which is what a recipient
    /// who has since opted out or a vendor whose credentials went away leaves.
    pub async fn abandon_attempt(&self, id: Uuid, why: &str) -> Result<(), DbError> {
        self.exec(
            "UPDATE type::record('delivery_attempt', <uuid>$id) \
             SET status = 'dead', attempts = 1, error = $error, next_retry_at = $now, \
                 update_time = $now \
             WHERE status = 'queued'",
            params!(id = id.to_string(), error = why, now = ts(Utc::now())),
        )
        .await
    }

    /// Rows whose retry time has arrived but whose claimer never came back.
    /// A probe: idle ticks match no rows, so it must not wake live readers.
    pub async fn requeue_stalled_attempts(
        &self,
        stalled_before: DateTime<Utc>,
    ) -> Result<(), DbError> {
        self.exec_quiet(
            "UPDATE delivery_attempt SET status = 'queued', update_time = $now \
             WHERE status = 'sending' AND update_time < $before",
            params!(now = ts(Utc::now()), before = ts(stalled_before)),
        )
        .await
    }
}

/// The tokens one device registered. A platform nobody has signed in on comes
/// back as an empty string, which is what an unregistered platform looks like.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PushTokens {
    #[serde(default)]
    pub push_token_ios: String,
    #[serde(default)]
    pub push_token_android: String,
}

impl Queries {
    pub async fn push_tokens(&self, user_id: Uuid) -> Result<Vec<PushTokens>, DbError> {
        self.all(
            "SELECT push_token_ios, push_token_android FROM user_device \
             WHERE user_id = $user ORDER BY id ASC",
            params!(user = user_id.to_string()),
        )
        .await
    }
}
