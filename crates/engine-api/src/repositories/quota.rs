#![allow(unused_imports, dead_code)]

use super::db::{Queries, affected, params, ts};
use super::models::PlayerQuota;
use crate::db::{DbError, Key};
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// The row one account's spend in one scope lives under. Deterministic, so a
/// charge reads and writes its own row without searching for it.
pub(crate) fn quota_id(user_id: Uuid, scope: &str) -> String {
    format!("{user_id}:{scope}")
}

impl Queries {
    pub(crate) async fn quota_row(
        &self,
        user_id: Uuid,
        scope: &str,
    ) -> Result<Option<PlayerQuota>, DbError> {
        self.opt(
            "SELECT * FROM type::record('player_quota', $id) LIMIT 1",
            params!(id = quota_id(user_id, scope)),
        )
        .await
    }

    /// Opens a scope's first window, reporting `false` when another request
    /// created the same row in the instant between the read and this write.
    pub(crate) async fn open_quota(&self, row: &PlayerQuota) -> Result<bool, DbError> {
        match self
            .create("player_quota", Key::String(row.id.clone()), row)
            .await
        {
            Ok(_) => Ok(true),
            Err(err) if err.is_unique() => Ok(false),
            Err(err) => Err(err),
        }
    }

    /// Spends inside the window the caller read. The guard is the SQL that
    /// counts: a concurrent charge that filled the cap, or a roll that expired
    /// the window, answers with no rows.
    pub(crate) async fn charge_window(
        &self,
        user_id: Uuid,
        scope: &str,
        amount: u32,
        cap: u32,
        boundary: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<bool, DbError> {
        let rows = affected(
            self,
            "UPDATE type::record('player_quota', $id) \
               SET used = used + $amount, cap = $cap, update_time = $now \
               WHERE used + $amount <= $cap AND window_start > $boundary \
               RETURN AFTER",
            params!(
                id = quota_id(user_id, scope),
                amount = amount,
                cap = cap,
                boundary = ts(boundary),
                now = ts(now),
            ),
        )
        .await?;
        Ok(rows.rows_affected() > 0)
    }

    /// Opens the next window and spends in it. `window_start <= $boundary` is
    /// the compare-and-swap: whoever rolled first moved the row past the
    /// boundary, and a second roll then writes nothing.
    pub(crate) async fn roll_window(
        &self,
        user_id: Uuid,
        scope: &str,
        amount: u32,
        cap: u32,
        period_secs: u64,
        boundary: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<bool, DbError> {
        let rows = affected(
            self,
            "UPDATE type::record('player_quota', $id) \
               SET used = $amount, cap = $cap, period_secs = $period, \
                   window_start = $now, update_time = $now \
               WHERE window_start <= $boundary \
               RETURN AFTER",
            params!(
                id = quota_id(user_id, scope),
                amount = amount,
                cap = cap,
                period = period_secs,
                boundary = ts(boundary),
                now = ts(now),
            ),
        )
        .await?;
        Ok(rows.rows_affected() > 0)
    }
}
