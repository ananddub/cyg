use super::*;
use engine_api::infra::console::{self, ConsoleRole};

/// The plane with a tight per-account bucket and no window tuned, so what a
/// request trips on is the bucket and never a durable allowance.
pub fn bucket_plane(exempt_console: bool) -> impl Fn(u16) -> ControlConfig {
    move |port| {
        let mut control = quota_plane(vec![])(port);
        control.quotas.exempt_console = exempt_console;
        control.quotas.player_rps = 1;
        control.quotas.player_burst = 2;
        control
    }
}

async fn ask(env: &Harness, token: &str, path: &str) -> (StatusCode, Option<u32>) {
    let resp = env
        .client
        .get(format!("{}{}", env.base, path))
        .bearer_auth(token)
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let wait = resp
        .headers()
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok());
    (status, wait)
}

/// A console token for an operator with an account id, which is the only kind
/// the plane can bucket.
fn operator_token() -> String {
    console::mint(
        &Uuid::new_v4().to_string(),
        "e2e-operator",
        "operator@console.local",
        ConsoleRole::Admin,
    )
}

pub async fn bucket_surface(env: &Harness) {
    eprintln!("gap: the per-account request bucket");
    let player = device_player(&env.client, &env.base, "burst").await;
    let room = format!("2...e2e-bucket-{}", letters_suffix());
    let path = format!("/v2/channel/{room}");

    for _ in 1..=2 {
        assert_eq!(
            ask(env, &player.token, &path).await.0,
            StatusCode::OK,
            "the burst is there to be spent"
        );
    }
    let (status, wait) = ask(env, &player.token, &path).await;
    assert_eq!(
        status,
        StatusCode::TOO_MANY_REQUESTS,
        "a third ask is faster than the bucket"
    );
    assert_eq!(wait, Some(1), "the bucket says how long to wait");

    tokio::time::sleep(Duration::from_millis(1_500)).await;
    assert_eq!(
        ask(env, &player.token, &path).await.0,
        StatusCode::OK,
        "the bucket refills"
    );

    let mate = device_player(&env.client, &env.base, "mate").await;
    for _ in 1..=2 {
        assert_eq!(
            ask(env, &mate.token, &path).await.0,
            StatusCode::OK,
            "one account's speed cannot spend another's"
        );
    }

    let operator = operator_token();
    for _ in 1..=6 {
        assert_eq!(
            ask(env, &operator, "/v2/console/user").await.0,
            StatusCode::OK,
            "an exempt operator is not the limiter's to judge"
        );
    }
}

pub async fn operator_bucket_surface(env: &Harness) {
    eprintln!("gap: an operator without the exemption");
    let operator = operator_token();
    for _ in 1..=2 {
        assert_eq!(
            ask(env, &operator, "/v2/console/user").await.0,
            StatusCode::OK,
            "the burst is there to be spent"
        );
    }
    let (status, wait) = ask(env, &operator, "/v2/console/user").await;
    assert_eq!(
        status,
        StatusCode::TOO_MANY_REQUESTS,
        "the config dropped the exemption, so an operator is bucketed"
    );
    assert_eq!(wait, Some(1), "the bucket says how long to wait");

    let player = device_player(&env.client, &env.base, "unbound").await;
    let room = format!("2...e2e-opbucket-{}", letters_suffix());
    assert_eq!(
        ask(env, &player.token, &format!("/v2/channel/{room}"))
            .await
            .0,
        StatusCode::OK,
        "the operator's bucket is not the player's"
    );
}
