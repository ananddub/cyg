use super::*;

pub async fn seed_leaderboard(db: &Datastore, id: &str, tournament: bool) {
    let now = chrono::Utc::now();
    let row = json!({
        "authoritative": false,
        "sort_order": 1,
        "operator": 0,
        "reset_schedule": Value::Null,
        "metadata": {},
        "create_time": engine_api::db::time::encode(now),
        "category": if tournament { 1 } else { 0 },
        "description": "",
        "duration": if tournament { 60 } else { 0 },
        "end_time": if tournament {
            engine_api::db::time::encode(now + chrono::Duration::hours(1))
        } else {
            engine_api::db::time::EPOCH.to_owned()
        },
        "join_required": false,
        "max_size": 100_000_000,
        "max_num_score": 1_000_000,
        "title": id,
        "size": 0,
        "start_time": engine_api::db::time::encode(now),
        "enable_ranks": true,
        "game_id": Value::Null,
    });
    exec(
        db,
        "CREATE type::record('leaderboard', $id) CONTENT $row RETURN NOTHING",
        &[("id", json!(id)), ("row", row)],
    )
    .await;
}
