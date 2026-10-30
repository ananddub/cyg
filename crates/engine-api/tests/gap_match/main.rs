use std::time::Duration;

use tokio::sync::mpsc;
use uuid::Uuid;

#[path = "../harness/mod.rs"]
mod harness_shared;

pub use harness_shared::*;

mod badge_surface;
mod result;

pub use badge_surface::*;
pub use result::*;

/// A player seated in a live match: the result endpoint credits only these.
#[derive(Clone)]
pub struct Seat {
    pub uid: Uuid,
    pub username: String,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
pub async fn match_results_stats_and_badges_end_to_end() {
    let env = Harness::start().await;
    let console = console_login(&env.client, &env.base).await;
    let host = device_player(&env.client, &env.base, "host").await;
    let away = device_player(&env.client, &env.base, "away").await;
    let outsider = device_player(&env.client, &env.base, "outsider").await;

    let board = format!("board-{}", letters_suffix());
    seed_leaderboard(env.db(), &board, false).await;

    result_surface(&env, &console, &host, &away, &outsider, &board).await;
    badge_surface(&env, &console, &host).await;

    tokio::time::sleep(Duration::from_millis(200)).await;
    env.stop().await;
    eprintln!("gap: match results, counters and badges passed");
}

pub fn seat(player: &Player) -> Seat {
    Seat {
        uid: Uuid::parse_str(&player.uid).expect("a player id"),
        username: player.username.clone(),
    }
}

/// Seats players in the control plane's live match registry, which is the only
/// record of who was in a match. No socket is needed: joining reads the session
/// it is handed and keeps a copy of the presence, so the sessions may drop here.
pub fn seat_match(who: &[Seat]) -> Uuid {
    use engine_api::infra::ws::session::Session;
    use engine_api::services::realtime::RealtimeMatchService;

    let service = RealtimeMatchService::global();
    let sessions: Vec<_> = who
        .iter()
        .map(|one| {
            let (tx, _rx) = mpsc::unbounded_channel();
            Session::new(Uuid::new_v4(), one.uid, one.username.clone(), tx)
        })
        .collect();
    let id = service.create_match(&sessions[0], true, 20, "e2e-match".to_owned());
    for session in &sessions {
        service
            .join_match(session, Some(id.simple().to_string()), None, None, None)
            .expect("seat a player in the match");
    }
    id
}

/// The counters a player holds, as the database sees them.
pub async fn counter_of(env: &Harness, uid: Uuid, stat: &str) -> Option<i64> {
    row(
        env.db(),
        "SELECT * FROM type::record('player_stat', $id) LIMIT 1",
        &[("id", json!(format!("{uid}:{stat}")))],
    )
    .await
    .map(|record| record["value"].as_i64().expect("a counter value"))
}

pub async fn board_rows(env: &Harness, board: &str, owner: Uuid) -> Vec<Value> {
    rows(
        env.db(),
        "SELECT * FROM leaderboard_record WHERE leaderboard_id = $board AND owner_id = $owner",
        &[("board", json!(board)), ("owner", json!(owner.to_string()))],
    )
    .await
}
