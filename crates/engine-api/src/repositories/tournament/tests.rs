use chrono::{Duration, Utc};
use serde_json::json;

use super::*;
use crate::repositories::leaderboard::{DefineLeaderboardParams, UpdateTournamentParams};
use crate::tests::games::{db, skipped_without_database};

#[tokio::test]
async fn only_a_tournament_category_board_reads() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    q.define_leaderboard(&DefineLeaderboardParams {
        id: "gap-cup".into(),
        title: "Gap Cup".into(),
        description: String::new(),
        operator: 0,
        sort_order: 1,
        reset_schedule: String::new(),
        metadata: json!({}),
        authoritative: false,
        category: 1,
        max_num_score: 1_000_000,
    })
    .await
    .expect("the tournament registers");
    let now = Utc::now();
    q.update_tournament(&UpdateTournamentParams {
        id: "gap-cup".into(),
        title: "Gap Cup".into(),
        description: String::new(),
        operator: 0,
        sort_order: 1,
        reset_schedule: None,
        metadata: json!({}),
        authoritative: false,
        duration: 3600,
        start_time: now,
        end_time: now + Duration::hours(1),
        join_required: false,
        max_size: 100,
        max_num_score: 1_000_000,
    })
    .await
    .expect("the tournament gets a duration");
    q.define_leaderboard(&DefineLeaderboardParams {
        id: "gap-plain".into(),
        title: "Gap Plain".into(),
        description: String::new(),
        operator: 0,
        sort_order: 1,
        reset_schedule: String::new(),
        metadata: json!({}),
        authoritative: false,
        category: 0,
        max_num_score: 1_000_000,
    })
    .await
    .expect("the plain board registers");

    let cup = q.get_tournament("gap-cup").await.expect("the cup reads");
    assert_eq!(cup.title, "Gap Cup");
    assert!(q.get_tournament("gap-plain").await.is_err());
    assert!(q.get_tournament("missing").await.is_err());
}
