#![allow(unused_imports, dead_code)]

use super::db::{Queries, params, ts};
use super::models::*;
use crate::db::{DbError, Key, time};
use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

const MAX_EVENT_LIMIT: i64 = 500;

fn bounded(limit: i64) -> i64 {
    limit.clamp(1, MAX_EVENT_LIMIT)
}

impl Queries {
    pub async fn record_gameplay_event(&self, row: &GameplayEvent) -> Result<(), DbError> {
        self.create("gameplay_event", Key::Uuid(row.id), row)
            .await
            .map(|_| ())
    }

    pub async fn record_desync_report(&self, row: &DesyncReport) -> Result<(), DbError> {
        self.create("desync_report", Key::Uuid(row.id), row)
            .await
            .map(|_| ())
    }

    /// Newest-first history for one room. A caller with no rows back is asking
    /// about a room that never listed an event, which is not an error.
    pub async fn list_gameplay_events(
        &self,
        room: &str,
        limit: i64,
    ) -> Result<Vec<GameplayEvent>, DbError> {
        self.all(
            "SELECT * FROM gameplay_event WHERE room = $room \
             ORDER BY create_time DESC LIMIT $limit",
            params!(room = room.to_string(), limit = bounded(limit)),
        )
        .await
    }

    /// Desync evidence, newest first, optionally narrowed to one room.
    pub async fn list_desync_reports(
        &self,
        room: Option<&str>,
        limit: i64,
    ) -> Result<Vec<DesyncReport>, DbError> {
        let limit = bounded(limit);
        match room.filter(|name| !name.is_empty()) {
            Some(name) => {
                self.all(
                    "SELECT * FROM desync_report WHERE room = $room \
                     ORDER BY create_time DESC LIMIT $limit",
                    params!(room = name.to_string(), limit = limit),
                )
                .await
            }
            None => {
                self.all(
                    "SELECT * FROM desync_report ORDER BY create_time DESC LIMIT $limit",
                    params!(limit = limit),
                )
                .await
            }
        }
    }

    /// The event stream and its desync evidence age out on one window: one
    /// without the other is a gap no operator can interpret.
    pub async fn prune_event_history(&self, older_than: DateTime<Utc>) -> Result<(), DbError> {
        self.exec(
            "DELETE gameplay_event WHERE create_time < $cutoff",
            params!(cutoff = ts(older_than)),
        )
        .await?;
        self.exec(
            "DELETE desync_report WHERE create_time < $cutoff",
            params!(cutoff = ts(older_than)),
        )
        .await
    }

    pub async fn record_match(&self, row: &MatchHistory) -> Result<(), DbError> {
        self.create("match_history", Key::Uuid(row.id), row)
            .await
            .map(|_| ())
    }

    /// One room's finished matches, newest first — the same room name runs a
    /// new match every few minutes, so a list and not a single row is correct.
    pub async fn list_match_history(
        &self,
        room: &str,
        limit: i64,
    ) -> Result<Vec<MatchHistory>, DbError> {
        self.all(
            "SELECT * FROM match_history WHERE room = $room \
             ORDER BY ended_at DESC LIMIT $limit",
            params!(room = room.to_string(), limit = bounded(limit)),
        )
        .await
    }

    pub async fn prune_match_history(&self, older_than: DateTime<Utc>) -> Result<(), DbError> {
        self.exec(
            "DELETE match_history WHERE ended_at < $cutoff",
            params!(cutoff = ts(older_than)),
        )
        .await
    }
}
