#![allow(unused_imports, dead_code)]

use serde_json::Value;

use super::*;

/// GDPR self-export reads. Each family is returned as raw records rather than a
/// model struct so the export document stays faithful to whatever is stored (and
/// cannot break when a table gains a field). Every query is capped.
impl Queries {
    pub async fn gdpr_purchases(&self, user_id: Uuid, limit: i64) -> Result<Vec<Value>, DbError> {
        self.all(
            "SELECT * FROM purchase WHERE user_id = $id LIMIT $limit",
            params!(id = user_id.to_string(), limit = limit),
        )
        .await
    }

    pub async fn gdpr_subscriptions(
        &self,
        user_id: Uuid,
        limit: i64,
    ) -> Result<Vec<Value>, DbError> {
        self.all(
            "SELECT * FROM subscription WHERE user_id = $id LIMIT $limit",
            params!(id = user_id.to_string(), limit = limit),
        )
        .await
    }

    pub async fn gdpr_inventory(&self, user_id: Uuid, limit: i64) -> Result<Vec<Value>, DbError> {
        self.all(
            "SELECT * FROM player_inventory_item WHERE user_id = $id LIMIT $limit",
            params!(id = user_id.to_string(), limit = limit),
        )
        .await
    }

    pub async fn gdpr_leaderboard_records(
        &self,
        user_id: Uuid,
        limit: i64,
    ) -> Result<Vec<Value>, DbError> {
        self.all(
            "SELECT * FROM leaderboard_record WHERE owner_id = $id LIMIT $limit",
            params!(id = user_id.to_string(), limit = limit),
        )
        .await
    }

    pub async fn gdpr_group_memberships(
        &self,
        user_id: Uuid,
        limit: i64,
    ) -> Result<Vec<Value>, DbError> {
        self.all(
            "SELECT * FROM group_edge WHERE source_id = $id OR destination_id = $id LIMIT $limit",
            params!(id = user_id.to_string(), limit = limit),
        )
        .await
    }

    pub async fn gdpr_notifications(
        &self,
        user_id: Uuid,
        limit: i64,
    ) -> Result<Vec<Value>, DbError> {
        self.all(
            "SELECT * FROM notification WHERE user_id = $id LIMIT $limit",
            params!(id = user_id.to_string(), limit = limit),
        )
        .await
    }

    pub async fn gdpr_messages(&self, user_id: Uuid, limit: i64) -> Result<Vec<Value>, DbError> {
        self.all(
            "SELECT * FROM message WHERE sender_id = $id OR stream_subject = $id \
             OR stream_descriptor = $id LIMIT $limit",
            params!(id = user_id.to_string(), limit = limit),
        )
        .await
    }
}
