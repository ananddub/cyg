#![allow(unused_imports, dead_code)]

use std::collections::BTreeMap;

use super::db::{Queries, params, ts};
use super::models::*;
use super::quest::player_stat_id;
use crate::db::{DbError, time};
use crate::utils::leaderboard::types::{DB_DECREMENT, DB_INCREMENT, DB_SET};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

/// One counter a reported result raises for one player.
pub(crate) struct CounterWrite {
    pub(crate) user_id: Uuid,
    pub(crate) stat: String,
    pub(crate) delta: i64,
}

/// One score a reported result posts to one leaderboard. `present` is whether
/// the owner already holds a record in this window: the board's operator merges
/// into what is there, and a first record takes the reported score as it stands.
pub(crate) struct ScoreWrite {
    pub(crate) leaderboard_id: String,
    pub(crate) owner_id: Uuid,
    pub(crate) expiry: DateTime<Utc>,
    pub(crate) score: i64,
    pub(crate) subscore: i64,
    pub(crate) username: Option<String>,
    pub(crate) metadata: Option<Value>,
    pub(crate) operator: i16,
    pub(crate) descending: bool,
    pub(crate) present: bool,
    pub(crate) max_num_score: i32,
}

/// The value a field holds now, or zero when the record never had one.
fn held(field: &str) -> String {
    format!("(IF {field} IS NONE THEN 0 ELSE {field} END)")
}

/// How the board's operator combines a reported score with the one on file.
fn merge(field: &str, param: &str, operator: i16, descending: bool) -> String {
    let current = held(field);
    match operator {
        DB_SET => format!("${param}"),
        DB_INCREMENT => format!("({current} + ${param})"),
        DB_DECREMENT => {
            format!("(IF {current} - ${param} < 0 THEN 0 ELSE {current} - ${param} END)")
        }
        _ => {
            let keeps = if descending { ">" } else { "<" };
            format!("(IF {current} {keeps} ${param} THEN {current} ELSE ${param} END)")
        }
    }
}

impl Queries {
    /// Writes a reported result as one fact: every counter and every score of
    /// the result move in a single transaction, so a result that cannot post its
    /// scores leaves no counters behind either. Counters are raised in the
    /// database, which is what makes a retried transaction the only way to apply
    /// this twice — an aborted one has raised nothing.
    pub(crate) async fn record_match_outcome(
        &self,
        counters: &[CounterWrite],
        scores: &[ScoreWrite],
    ) -> Result<(), DbError> {
        let now = Utc::now();
        let mut sql = String::from("BEGIN TRANSACTION;");
        let mut args: BTreeMap<String, Value> = BTreeMap::new();
        args.insert("now".to_owned(), json!(ts(now)));
        for (index, counter) in counters.iter().enumerate() {
            sql.push_str(&format!(
                "UPSERT type::record('player_stat', $stat_id{index}) \
                 SET user_id = $stat_user{index}, stat = $stat_name{index}, \
                     value = {} + $stat_delta{index}, update_time = $now \
                 RETURN NONE;",
                held("value")
            ));
            args.insert(
                format!("stat_id{index}"),
                json!(player_stat_id(counter.user_id, &counter.stat)),
            );
            args.insert(
                format!("stat_user{index}"),
                json!(counter.user_id.to_string()),
            );
            args.insert(format!("stat_name{index}"), json!(counter.stat.clone()));
            args.insert(format!("stat_delta{index}"), json!(counter.delta));
        }
        for (index, write) in scores.iter().enumerate() {
            let score = merge(
                "score",
                &format!("score{index}"),
                write.operator,
                write.descending,
            );
            let subscore = merge(
                "subscore",
                &format!("subscore{index}"),
                write.operator,
                write.descending,
            );
            sql.push_str(&format!(
                "UPDATE leaderboard_record SET score = {score}, subscore = {subscore}, \
                     num_score = {} + 1, update_time = $now, \
                     username = IF $name{index} IS NULL OR $name{index} IS NONE \
                                    THEN username ELSE $name{index} END, \
                     metadata = IF $meta{index} IS NULL OR $meta{index} IS NONE \
                                    THEN metadata ELSE $meta{index} END \
                 WHERE leaderboard_id = $board{index} AND owner_id = $owner{index} \
                   AND expiry_time = $expiry{index} AND $present{index} RETURN NONE;",
                held("num_score")
            ));
            sql.push_str(&format!(
                "INSERT INTO leaderboard_record ( \
                     SELECT $board{index} AS leaderboard_id, $owner{index} AS owner_id, \
                            $name{index} AS username, $score{index} AS score, \
                            $subscore{index} AS subscore, 1 AS num_score, \
                            $meta{index} AS metadata, $now AS create_time, $now AS update_time, \
                            $expiry{index} AS expiry_time, $max{index} AS max_num_score \
                     FROM type::record('leaderboard', $board{index}) WHERE $present{index} = false \
                 ) RETURN NONE;"
            ));
            args.insert(format!("board{index}"), json!(write.leaderboard_id.clone()));
            args.insert(format!("owner{index}"), json!(write.owner_id.to_string()));
            args.insert(format!("expiry{index}"), json!(ts(write.expiry)));
            args.insert(format!("score{index}"), json!(write.score));
            args.insert(format!("subscore{index}"), json!(write.subscore));
            args.insert(
                format!("name{index}"),
                json!(
                    write
                        .username
                        .clone()
                        .map(Value::from)
                        .unwrap_or(Value::Null)
                ),
            );
            args.insert(
                format!("meta{index}"),
                write.metadata.clone().unwrap_or(Value::Null),
            );
            args.insert(format!("max{index}"), json!(write.max_num_score));
            args.insert(format!("present{index}"), json!(write.present));
        }
        sql.push_str("COMMIT TRANSACTION;");
        self.db
            .query_transaction::<Value>(&sql, &args)
            .await
            .map(|_| ())
    }
}
