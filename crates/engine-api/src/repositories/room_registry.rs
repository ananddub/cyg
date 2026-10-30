#![allow(unused_imports, dead_code)]

use super::db::{Queries, params, ts};
use super::models::*;
use crate::db::{DbError, Key, time};
use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

/// The console's room table has the same ceiling the other operator lists do: a
/// fleet with more rooms than this is a fleet the reader paginates past anyway.
const MAX_REGISTRY_ROWS: i64 = 500;

/// How long a claim is trusted after its host last refreshed it. A live room
/// re-claims once a second, so ten of these is a pump that stopped rather than a
/// room that went — while still being short enough that a pod deleted without
/// warning stops sending players to a room that is not running.
pub fn lease() -> chrono::Duration {
    chrono::Duration::seconds(10)
}

/// Whether the owner stopped beating: the room is nobody's, and the name is free
/// again. Both the matchmake handoff and the console's room table ask this, so
/// they cannot disagree about a row they are looking at together.
pub fn lease_lapsed(row: &RoomRegistry, now: DateTime<Utc>) -> bool {
    now - row.update_time > lease()
}

impl Queries {
    /// Record who runs a room. Keyed by the room name, so a host's once-a-second
    /// claim is an update of its own row rather than a new one — which is what
    /// lets the lease be the row's `update_time` instead of a second table.
    pub async fn claim_room(&self, row: &RoomRegistry) -> Result<(), DbError> {
        self.exec(
            "UPSERT type::record('room_registry', $room_id) CONTENT $data",
            params!(room_id = row.room_id.clone(), data = row),
        )
        .await
    }

    /// Give the name back. A drain does this for every room it closes; a row that
    /// outlives its node by more than the lease is treated as released anyway, so
    /// this is the fast path rather than the only one.
    pub async fn release_room(&self, room_id: &str) -> Result<(), DbError> {
        self.exec(
            "DELETE type::record('room_registry', $room_id)",
            params!(room_id = room_id),
        )
        .await
    }

    pub async fn load_room_registry(
        &self,
        room_id: &str,
    ) -> Result<Option<RoomRegistry>, DbError> {
        self.opt(
            "SELECT * FROM type::record('room_registry', $room_id) LIMIT 1",
            params!(room_id = room_id),
        )
        .await
    }

    /// Every room the registry currently names an owner for, oldest claim first —
    /// a lease about to lapse is the row an operator wants at the top.
    pub async fn list_room_registry(&self) -> Result<Vec<RoomRegistry>, DbError> {
        self.all(
            "SELECT * FROM room_registry ORDER BY update_time ASC LIMIT $limit",
            params!(limit = MAX_REGISTRY_ROWS),
        )
        .await
    }
}
