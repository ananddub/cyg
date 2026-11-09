//! How often a rule has already paid out.
//!
//! A rule with no cooldown fires every time its trigger does, which is what an
//! operator wants for "grant 100 gems on purchase" and does not want for "grant
//! 100 gems when the player reaches level 5" — the second one would pay on every
//! level-bound event forever. A cooldown and a lifetime cap are the two things
//! that make a rule safe to leave published.
//!
//! The row is the ledger: one per (rule, player), carrying how many times the
//! rule has fired and when it last did. Reading it is one lookup, so the check
//! costs the same whether a rule has fired once or a thousand times.

use super::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("rule_fire", schemaless)]
#[index(
    name = "rule_fire_rule_user_uniq",
    fields = "rule_key, user_id",
    unique
)]
pub struct RuleFire {
    /// `<rule key>:<user id>` — one row per pair, so a fire is an upsert rather
    /// than a read-then-write, which two concurrent events would race.
    #[field(thing)]
    pub id: String,
    pub rule_key: String,
    pub user_id: Uuid,
    /// How many times this rule has paid this player.
    pub count: i64,
    /// When it last paid. `None` before the first fire.
    pub last_fire: Option<DateTime<Utc>>,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
}

impl RuleFire {
    pub fn record_id(rule_key: &str, user_id: Uuid) -> String {
        format!("{rule_key}:{user_id}")
    }
}
