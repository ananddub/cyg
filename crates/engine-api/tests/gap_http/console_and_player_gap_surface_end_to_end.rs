use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
pub async fn console_and_player_gap_surface_end_to_end() {
    let env = Harness::start().await;
    let console = console_login(&env.client, &env.base).await;

    dashboard_identity(&env).await;
    static_surface(&env).await;
    console_operator_lifecycle(&env, &console).await;
    console_account_lifecycle(&env, &console).await;
    let _group_id = console_group_export(&env, &console).await;
    console_match_surface(&env, &console).await;
    console_leaderboard_record_delete(&env, &console).await;
    player_surface(&env).await;

    tokio::time::sleep(Duration::from_millis(200)).await;
    env.stop().await;
    eprintln!("gap: every uncovered HTTP surface passed");
}
