use super::*;

pub fn letters_suffix() -> String {
    let mut value = Uuid::new_v4().as_u128();
    let mut out = String::new();
    while out.len() < 8 {
        out.push((b'a' + (value % 26) as u8) as char);
        value /= 26;
    }
    out
}
pub fn accepts_username(candidate: &str) -> bool {
    use rustrict::{Censor, Type};
    !Censor::from_str(candidate)
        .with_censor_threshold(Type::INAPPROPRIATE)
        .analyze()
        .is(Type::INAPPROPRIATE)
}
pub const CONSOLE_USER: &str = "e2e-admin";
pub const CONSOLE_PASSWORD: &str = "e2e-password";
pub const SERVER_KEY: &str = "e2e-server-key";
pub const GAME_NAME: &str = "e2e-game";
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
pub async fn rest_api_end_to_end() {
    let env = Harness::start().await;
    let base = env.base.clone();
    let client = env.client.clone();

    let port = env.port;
    telemetry(&client, &base, port).await;
    let seat_token = matchmake(&client, &base, port).await;
    let console = console_auth(&client, &base, &env.pool).await;
    let (relay_game, wasm_game) = games(&client, &base, port, &console).await;
    wasm_lifecycle(&client, &base, &console, &relay_game, &wasm_game).await;
    device_auth(&client, &base, &env.pool).await;
    auth_surface(&client, &base, &env.pool).await;
    account_surface(&client, &base, &env.pool).await;
    link_surface(&client, &base, &env.pool).await;
    user_surface(&client, &base, &env.pool).await;
    friend_surface(&client, &base, &env.pool).await;
    group_surface(&client, &base, &env.pool).await;
    channel_surface(&client, &base, &env.pool).await;
    notification_surface(&client, &base, &env.pool).await;
    storage_surface(&client, &base).await;
    storage_index_surface(&client, &base).await;
    inventory_surface(&client, &base).await;
    leaderboard_surface(&client, &base, &env.pool).await;
    iap_surface(&client, &base, &env.pool).await;
    tournaments(&client, &base, &console, &env.pool).await;
    audit_trail(&env.pool).await;
    assert!(!seat_token.is_empty());

    env.stop().await;
    eprintln!("e2e: all REST scenarios passed");
}
pub struct Harness {
    pub base: String,
    pub port: u16,
    pub client: Client,
    pub pool: Arc<Datastore>,
    pub shutdown: tokio::sync::oneshot::Sender<()>,
    pub server: tokio::task::JoinHandle<()>,
}
