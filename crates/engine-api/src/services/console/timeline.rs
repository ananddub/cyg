use axum::Json;
use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::ApiState;
use crate::dto::console::timeline::{TimelineEntry, TimelinePage, TimelineQuery};
use crate::error::V2Error;
use crate::services::deps::q;

const MAX_LIMIT: i64 = 200;
const DEFAULT_LIMIT: i64 = 50;

impl super::ConsoleService {
    /// One ordered feed over the player's ledger, inventory grants, purchases,
    /// subscriptions, notifications and sent messages. Sources that grow later
    /// (gameplay events, match history) join as one more fetch-and-map arm.
    pub async fn player_timeline(
        &self,
        state: &ApiState,
        account_id: Uuid,
        query: TimelineQuery,
    ) -> Result<Json<TimelinePage>, V2Error> {
        let queries = q(state).await?;
        if !queries.timeline_account_exists(account_id).await? {
            return Err(V2Error::not_found("That account does not exist."));
        }

        let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        // One extra row per source makes "a next page exists" decidable
        // without a second pass.
        let fetch = limit + 1;
        let cursor = query.before.as_deref().and_then(decode_cursor);
        let before_at = cursor.as_ref().map(|c| c.0);

        let mut entries = Vec::new();
        for ledger in queries
            .timeline_ledger(account_id, before_at, fetch, query.game_id)
            .await?
        {
            entries.push(TimelineEntry {
                id: ledger.id.to_string(),
                kind: "wallet".to_owned(),
                at: ledger.create_time,
                summary: "Wallet change".to_owned(),
                detail: serde_json::to_value(&ledger).unwrap_or(Value::Null),
            });
        }
        for item in queries
            .timeline_inventory(account_id, before_at, fetch)
            .await?
        {
            entries.push(TimelineEntry {
                id: item.instance_id.to_string(),
                kind: "inventory".to_owned(),
                at: item.created_at,
                summary: format!("Received {} × {}", item.item_id, item.quantity),
                detail: serde_json::to_value(&item).unwrap_or(Value::Null),
            });
        }
        for purchase in queries
            .timeline_purchases(account_id, before_at, fetch)
            .await?
        {
            entries.push(TimelineEntry {
                id: purchase.transaction_id.clone(),
                kind: "purchase".to_owned(),
                at: purchase.create_time,
                summary: format!(
                    "Purchased {} via {} in {}",
                    purchase.product_id,
                    store_name(purchase.store),
                    environment_name(purchase.environment),
                ),
                detail: serde_json::to_value(&purchase).unwrap_or(Value::Null),
            });
        }
        for subscription in queries
            .timeline_subscriptions(account_id, before_at, fetch)
            .await?
        {
            entries.push(TimelineEntry {
                id: subscription.original_transaction_id.clone(),
                kind: "subscription".to_owned(),
                at: subscription.create_time,
                summary: format!("Subscription {} recorded", subscription.product_id),
                detail: serde_json::to_value(&subscription).unwrap_or(Value::Null),
            });
        }
        for notification in queries
            .timeline_notifications(account_id, before_at, fetch)
            .await?
        {
            entries.push(TimelineEntry {
                id: notification.id.to_string(),
                kind: "notification".to_owned(),
                at: notification.create_time,
                summary: notification.subject.clone(),
                detail: serde_json::to_value(&notification).unwrap_or(Value::Null),
            });
        }
        for message in queries
            .timeline_messages(account_id, before_at, fetch)
            .await?
        {
            entries.push(TimelineEntry {
                id: message.id.to_string(),
                kind: "message".to_owned(),
                at: message.create_time,
                summary: format!("Sent a message in stream {}", message.stream_subject),
                detail: serde_json::to_value(&message).unwrap_or(Value::Null),
            });
        }

        entries.sort_by_key(|entry| std::cmp::Reverse(sort_key(entry)));
        if let Some(cursor) = &cursor {
            let key = (cursor.0, cursor.1.clone(), cursor.2.clone());
            entries.retain(|entry| sort_key(entry) < key);
        }
        let next_cursor = if entries.len() > limit as usize {
            entries.get(limit as usize - 1).map(encode_cursor)
        } else {
            None
        };
        entries.truncate(limit as usize);
        Ok(Json(TimelinePage {
            account_id,
            entries,
            next_cursor,
        }))
    }
}

fn sort_key(entry: &TimelineEntry) -> (DateTime<Utc>, String, String) {
    (entry.at, entry.kind.clone(), entry.id.clone())
}

/// The cursor is the last entry the page kept; the next page starts strictly
/// after it in the merged order, so a cursor is stable across inserts.
fn encode_cursor(entry: &TimelineEntry) -> String {
    format!(
        "{}:{}:{}",
        entry.at.timestamp_millis(),
        entry.kind,
        entry.id
    )
}

fn decode_cursor(raw: &str) -> Option<(DateTime<Utc>, String, String)> {
    let mut parts = raw.splitn(3, ':');
    let millis = parts.next()?.parse::<i64>().ok()?;
    let kind = parts.next()?.to_owned();
    let id = parts.next()?.to_owned();
    Some((DateTime::from_timestamp_millis(millis)?, kind, id))
}

fn store_name(code: i16) -> &'static str {
    match code {
        0 => "Apple",
        1 => "Google",
        2 => "Huawei",
        3 => "Facebook Instant",
        4 => "Samsung",
        _ => "Unknown store",
    }
}

fn environment_name(code: i16) -> &'static str {
    match code {
        1 => "SANDBOX",
        2 => "PRODUCTION",
        _ => "UNKNOWN",
    }
}
