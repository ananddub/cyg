use super::*;

pub async fn wallet_idempotency(env: &Harness, console: &str) {
    eprintln!("gap: idempotent wallet writes");
    let player = device_player(&env.client, &env.base, "wallet").await;
    let url = format!("{}/v2/console/account/{}/wallet", env.base, player.uid);

    let first = json_body(
        env.client
            .post(&url)
            .bearer_auth(console)
            .header("Idempotency-Key", "wallet-1")
            .json(&json!({"changeset": {"gold": 10}, "metadata": {"why": "signup bonus"}}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "wallet credit with an idempotency key",
    )
    .await;
    assert_eq!(
        first["wallet"]["gold"],
        json!(10.0),
        "the credit lands: {first}"
    );

    let replay = json_body(
        env.client
            .post(&url)
            .bearer_auth(console)
            .header("Idempotency-Key", "wallet-1")
            .json(&json!({"changeset": {"gold": 10}, "metadata": {"why": "signup bonus"}}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "replaying the wallet credit",
    )
    .await;
    assert_eq!(
        replay["wallet"]["gold"],
        json!(10.0),
        "the replay returns the stored response without re-executing: {replay}"
    );

    let ledger = rows_matching(env, "wallet_ledger", &player.uid).await;
    assert_eq!(
        ledger.len(),
        1,
        "the replay wrote no second ledger row: {ledger:?}"
    );

    let text = refusal(
        env.client
            .post(&url)
            .bearer_auth(console)
            .header("Idempotency-Key", "wallet-1")
            .json(&json!({"changeset": {"gold": 999}}))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "reusing the key for a different changeset",
    )
    .await;
    assert!(
        text.contains("already used for a different request"),
        "the reuse refusal names the collision: {text}"
    );

    let fire = || {
        let client = env.client.clone();
        let url = url.clone();
        let token = console.to_owned();
        async move {
            client
                .post(&url)
                .bearer_auth(&token)
                .header("Idempotency-Key", "wallet-2")
                .json(&json!({"changeset": {"gold": 5}, "metadata": {}}))
                .send()
                .await
                .unwrap()
                .status()
        }
    };
    let (a, b) = tokio::join!(fire(), fire());
    let accepted = [a, b]
        .iter()
        .filter(|status| **status == StatusCode::OK)
        .count();
    let in_flight = [a, b]
        .iter()
        .filter(|status| **status == StatusCode::CONFLICT)
        .count();
    assert_eq!(
        accepted + in_flight,
        2,
        "a raced double submit answers 200 and 409/200, never 5xx: {a} {b}"
    );
    assert!(
        accepted >= 1,
        "one of the concurrent posts executed: {a} {b}"
    );

    let ledger = rows_matching(env, "wallet_ledger", &player.uid).await;
    assert_eq!(
        ledger.len(),
        2,
        "the concurrent double submit granted exactly once: {ledger:?}"
    );

    let account = json_body(
        env.client
            .get(format!("{}/v2/console/account/{}", env.base, player.uid))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the console reads the player account row",
    )
    .await;
    assert_eq!(
        account["wallet"]["gold"],
        json!(15.0),
        "the two executed credits summed, the replay and the loser did not: {account}"
    );
}
