use chrono::{Duration, Utc};
use serde_json::json;
use uuid::Uuid;

use super::*;
use crate::tests::games::{db, skipped_without_database};

#[tokio::test]
async fn a_board_definition_upserts_and_a_record_reads_back() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let board = "gap-board";
    q.define_leaderboard(&DefineLeaderboardParams {
        id: board.into(),
        title: "Weekly".into(),
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
    .expect("the board registers");

    let row = q.get_leaderboard(board).await.expect("the board reads");
    assert_eq!(row.title, "Weekly");

    let owner = Uuid::new_v4();
    let now = Utc::now();
    let expiry = now + Duration::days(1);
    let record = LeaderboardRecord {
        leaderboard_id: board.into(),
        owner_id: owner,
        username: Some("alice".into()),
        score: 10,
        subscore: 1,
        num_score: 1,
        metadata: json!({}),
        create_time: now,
        update_time: now,
        expiry_time: expiry,
        max_num_score: 1_000_000,
    };
    q.create_leaderboard_record(&record)
        .await
        .expect("the record stores");
    assert_eq!(
        q.count_leaderboard_records(board, expiry)
            .await
            .expect("the count runs"),
        1
    );
    let stored = q
        .leaderboard_record_for_write(board, owner, expiry)
        .await
        .expect("the record reads")
        .expect("the record exists");
    assert_eq!(stored.score, 10);
}

#[tokio::test]
async fn deleting_a_board_takes_its_records() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let board = "gap-board-delete";
    q.define_leaderboard(&DefineLeaderboardParams {
        id: board.into(),
        title: "Doomed".into(),
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
    .expect("the board registers");

    let removed = q.delete_board(board).await.expect("the board deletes");
    assert_eq!(removed.len(), 1);
    assert!(q.get_leaderboard(board).await.is_err());
    q.delete_board_records(board)
        .await
        .expect("the record sweep runs");
}
