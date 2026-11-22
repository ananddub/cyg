use std::collections::BTreeMap;

use serde_json::json;

use crate::db::{DbError, time};
use crate::repositories::db::{Queries, params};
use crate::repositories::models::{PlayerStat, StatWriteReceipt};
use crate::repositories::quest::player_stat_id;

impl Queries {
    /// Applies one host-validated counter increment exactly once.
    ///
    /// The receipt row and the counter move in the same transaction: a
    /// redelivered `write_id` finds the receipt and leaves `player_stat`
    /// untouched, so a retried bus frame cannot double-count. Returns the new
    /// counter total when this call applied, `None` when it was a duplicate.
    pub(crate) async fn apply_stat_write(
        &self,
        row: &StatWriteReceipt,
    ) -> Result<Option<i64>, DbError> {
        let stat_id = player_stat_id(row.user_id, &row.stat);
        let now = time::encode(row.create_time);
        let receipt = json!({
            "id": row.id,
            "room": row.room,
            "tick": row.tick,
            "user_id": row.user_id.to_string(),
            "stat": row.stat,
            "delta": row.delta,
            "create_time": now,
        });
        // Creating the receipt first makes the uniqueness check the
        // transaction's gate: a duplicate `write_id` fails before the counter
        // moves, and the whole transaction aborts, so nothing is applied twice.
        let surql = "BEGIN TRANSACTION; \
            CREATE type::record('stat_write', $write_id) CONTENT $receipt RETURN NONE; \
            UPSERT type::record('player_stat', $stat_id) \
              SET user_id = $user, stat = $stat, \
                  value = (IF value IS NONE THEN 0 ELSE value END) + $delta, \
                  update_time = $now \
              RETURN AFTER; \
            COMMIT TRANSACTION;";
        let params: BTreeMap<String, serde_json::Value> = params!(
            write_id = row.id.clone(),
            stat_id = stat_id,
            user = row.user_id.to_string(),
            stat = row.stat.clone(),
            delta = row.delta,
            now = now,
            receipt = receipt,
        );
        let rows: Vec<PlayerStat> = match self.db.query_transaction(surql, &params).await {
            Ok(rows) => rows,
            Err(DbError::Unique(_)) => return Ok(None),
            Err(err) => return Err(err),
        };
        Ok(rows.into_iter().next().map(|stat| stat.value))
    }

    /// Drops idempotency receipts older than the retry window. The counter rows
    /// are the history; a receipt only has to outlive a redelivered bus frame.
    pub(crate) async fn prune_stat_writes(
        &self,
        older_than: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), DbError> {
        self.exec(
            "DELETE stat_write WHERE create_time < $cutoff",
            params!(cutoff = crate::repositories::db::ts(older_than)),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::Utc;
    use uuid::Uuid;

    use super::*;
    use crate::db::{Datastore, KvBudget};

    async fn queries() -> Queries {
        let db = Arc::new(
            Datastore::open("mem://", "stat", "stat", None, None, KvBudget::default())
                .await
                .expect("the embedded store opens"),
        );
        Queries::new(db)
    }

    fn receipt(id: &str, user: Uuid, delta: i64) -> StatWriteReceipt {
        StatWriteReceipt {
            id: id.to_string(),
            room: "arena".to_string(),
            tick: 7,
            user_id: user,
            stat: "kills".to_string(),
            delta,
            create_time: Utc::now(),
        }
    }

    #[tokio::test]
    async fn a_redelivered_write_id_does_not_double_count() {
        let queries = queries().await;
        let user = Uuid::new_v4();

        assert_eq!(
            queries
                .apply_stat_write(&receipt("boot:1", user, 3))
                .await
                .expect("first write applies"),
            Some(3)
        );
        assert_eq!(
            queries
                .apply_stat_write(&receipt("boot:1", user, 3))
                .await
                .expect("a duplicate is not an error"),
            None
        );
        assert_eq!(
            queries
                .get_player_stat(user, "kills")
                .await
                .expect("the counter is readable")
                .expect("the counter exists")
                .value,
            3
        );

        assert_eq!(
            queries
                .apply_stat_write(&receipt("boot:2", user, 2))
                .await
                .expect("the next write applies"),
            Some(5)
        );
        assert_eq!(
            queries
                .get_player_stat(user, "kills")
                .await
                .expect("the counter is readable")
                .expect("the counter exists")
                .value,
            5
        );
    }
}
