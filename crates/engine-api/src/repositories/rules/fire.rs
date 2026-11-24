//! Reading and updating how often a rule has paid a player.

use super::*;
use chrono::Utc;

impl Queries {
    pub async fn get_rule_fire(
        &self,
        rule_key: &str,
        user_id: Uuid,
    ) -> Result<Option<RuleFire>, DbError> {
        self.opt(
            "SELECT * FROM type::record('rule_fire', $id) LIMIT 1",
            params!(id = RuleFire::record_id(rule_key, user_id)),
        )
        .await
    }

    /// Record a fire and return the row it produced.
    ///
    /// `count` is incremented in the statement rather than computed in Rust: two
    /// events for the same player can arrive together, and a read-then-write
    /// would let the second overwrite the first's increment — which is exactly
    /// the case a lifetime cap exists to catch.
    pub async fn record_rule_fire(
        &self,
        rule_key: &str,
        user_id: Uuid,
    ) -> Result<RuleFire, DbError> {
        let now = Utc::now();
        self.one(
            "UPSERT type::record('rule_fire', $id) SET \
               rule_key = $rule_key, user_id = $user_id, \
               count = count + 1, last_fire = $now, update_time = $now \
             RETURN AFTER",
            params!(
                id = RuleFire::record_id(rule_key, user_id),
                rule_key = rule_key,
                user_id = user_id.to_string(),
                now = crate::repositories::db::ts(now),
            ),
        )
        .await
    }
}
