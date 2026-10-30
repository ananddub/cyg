use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
pub async fn player_groups_boards_channels_iap_and_social_end_to_end() {
    let env = Harness::start().await;
    let console = console_login(&env.client, &env.base).await;

    group_surface(&env).await;
    leaderboard_surface(&env).await;
    tournament_surface(&env, &console).await;
    storage_index_surface(&env).await;
    channel_surface(&env).await;
    iap_surface(&env).await;
    provider_auth_surface(&env).await;
    link_unlink_surface(&env).await;
    report_surface(&env, &console).await;

    env.stop().await;
    eprintln!(
        "gap: player group, leaderboard, tournament, index, channel, iap and social surfaces passed"
    );
}
#[allow(dead_code)]
pub fn _stamp_marker(t: DateTime<Utc>) -> String {
    t.to_rfc3339()
}
