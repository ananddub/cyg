use super::*;

/// A refusal as the wire shows it: the body, its message, and the wait the
/// plane promises. The harness helpers drop headers, and the wait is the point
/// of this surface.
struct Refusal {
    body: Value,
    message: String,
    retry_after: u32,
}

impl Refusal {
    async fn new(resp: reqwest::Response, status: StatusCode, what: &str) -> Self {
        let actual = resp.status();
        let wait = resp
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let body = resp.text().await.unwrap_or_default();
        assert_eq!(
            actual, status,
            "{what}: expected {status}, got {actual}: {body}"
        );
        let parsed: Value = serde_json::from_str(&body)
            .unwrap_or_else(|err| panic!("{what}: refusal is not JSON ({err}): {body}"));
        let message = parsed["error"]["message"]
            .as_str()
            .unwrap_or_else(|| panic!("{what}: refusal has no message: {body}"))
            .to_owned();
        let retry_after = wait
            .parse()
            .unwrap_or_else(|_| panic!("{what}: refusal promises no wait: {body}"));
        Refusal {
            body: parsed,
            message,
            retry_after,
        }
    }
}

async fn send(
    env: &Harness,
    token: &str,
    room: &str,
    line: &str,
) -> Result<Value, reqwest::Response> {
    let resp = env
        .client
        .post(format!("{}/v2/channel/{room}/message", env.base))
        .bearer_auth(token)
        .json(&json!({ "content": { "text": line } }))
        .send()
        .await
        .unwrap();
    if resp.status() == StatusCode::OK {
        return Ok(json_body(resp, StatusCode::OK, "a send inside the allowance").await);
    }
    Err(resp)
}

async fn put_batch(
    env: &Harness,
    token: &str,
    collection: &str,
    count: usize,
) -> reqwest::Response {
    let objects: Vec<_> = (0..count)
        .map(|n| {
            json!({
                "collection": collection,
                "key": format!("gap-k{n}"),
                "value": format!("{{\"n\": {n}}}"),
            })
        })
        .collect();
    env.client
        .put(format!("{}/v2/storage", env.base))
        .bearer_auth(token)
        .json(&json!({ "objects": objects }))
        .send()
        .await
        .unwrap()
}

async fn window_of(env: &Harness, uid: &str, scope: &str) -> Value {
    row(
        env.db(),
        "SELECT * FROM player_quota WHERE user_id = $uid AND scope = $scope LIMIT 1",
        &[("uid", json!(uid)), ("scope", json!(scope))],
    )
    .await
    .unwrap_or_else(|| panic!("the {scope} window for {uid}"))
}

pub async fn durable_surface(env: &Harness) {
    eprintln!("gap: durable quota windows");
    let author = device_player(&env.client, &env.base, "spent").await;
    let mate = device_player(&env.client, &env.base, "fresh").await;
    let room = format!("2...e2e-window-{}", letters_suffix());

    // Opening each account spent a unit of its own sign-in window, so the scope
    // the plan charges at login is measured without a second route.
    let logins = window_of(env, &author.uid, LOGIN).await;
    assert_eq!(logins["used"], json!(1), "{logins}");
    assert_eq!(logins["cap"], json!(20), "{logins}");

    for line in 1..=3 {
        let body = send(env, &author.token, &room, &format!("gap line {line}"))
            .await
            .unwrap_or_else(|_| panic!("send {line} fits the window"));
        assert_eq!(body["sender_id"], author.uid);
    }

    let refusal = Refusal::new(
        send(env, &author.token, &room, "gap fourth line")
            .await
            .err()
            .expect("a fourth send is over the window"),
        StatusCode::TOO_MANY_REQUESTS,
        "a send past the allowance",
    )
    .await;
    assert_eq!(refusal.body["error"]["code"], json!(8), "{}", refusal.body);
    assert_eq!(
        refusal.message,
        "This account has used its allowance for this window."
    );
    assert!(
        (1..=300).contains(&refusal.retry_after),
        "the wait is a third of a period, got {}",
        refusal.retry_after
    );

    let spent = window_of(env, &author.uid, CHANNEL_SEND).await;
    assert_eq!(spent["used"], json!(3), "{spent}");
    assert_eq!(spent["cap"], json!(3), "{spent}");
    assert_eq!(spent["period_secs"], json!(300), "{spent}");

    let neighbour = send(env, &mate.token, &room, "a neighbour's line")
        .await
        .expect("one account's window cannot spend another's");
    assert_eq!(neighbour["sender_id"], mate.uid);
    assert_eq!(
        window_of(env, &mate.uid, CHANNEL_SEND).await["used"],
        json!(1),
        "the neighbour opened its own window"
    );
}

pub async fn rollover_surface(env: &Harness) {
    eprintln!("gap: quota windows roll over");
    let author = device_player(&env.client, &env.base, "turn").await;
    let room = format!("2...e2e-turn-{}", letters_suffix());

    send(env, &author.token, &room, "first of the period")
        .await
        .expect("the period starts open");
    let refusal = Refusal::new(
        send(env, &author.token, &room, "too soon again")
            .await
            .err()
            .expect("the period is not over"),
        StatusCode::TOO_MANY_REQUESTS,
        "a second send inside a one-second window",
    )
    .await;
    assert_eq!(
        refusal.message,
        "This account has used its allowance for this window."
    );

    let closed = window_of(env, &author.uid, CHANNEL_SEND).await;
    let started = closed["window_start"]
        .as_str()
        .expect("a window start")
        .to_owned();
    assert_eq!(closed["used"], json!(1), "{closed}");

    tokio::time::sleep(Duration::from_secs(2)).await;
    send(env, &author.token, &room, "the next period")
        .await
        .unwrap_or_else(|resp| panic!("{}: the window never opened again", resp.status()));

    let rolled = window_of(env, &author.uid, CHANNEL_SEND).await;
    assert_eq!(rolled["used"], json!(1), "{rolled}");
    assert_ne!(
        rolled["window_start"].as_str().expect("a window start"),
        started.as_str(),
        "the rolled row records a new window"
    );
}

pub async fn batch_surface(env: &Harness) {
    eprintln!("gap: a storage batch spends its size");
    let author = device_player(&env.client, &env.base, "batch").await;
    let collection = format!("e2e_gap_quota_{}", letters_suffix());

    expect(
        put_batch(env, &author.token, &collection, 1).await,
        StatusCode::OK,
        "a write of one object",
    )
    .await;
    assert_eq!(
        window_of(env, &author.uid, STORAGE_WRITE).await["used"],
        json!(1),
        "one object spent one unit"
    );

    let refusal = Refusal::new(
        put_batch(env, &author.token, &collection, 9).await,
        StatusCode::TOO_MANY_REQUESTS,
        "a batch larger than the whole allowance",
    )
    .await;
    assert_eq!(
        refusal.message,
        "This request is larger than the account's whole allowance."
    );

    let refusal = Refusal::new(
        put_batch(env, &author.token, &collection, 2).await,
        StatusCode::TOO_MANY_REQUESTS,
        "a batch that fills past the cap",
    )
    .await;
    assert_eq!(
        refusal.message,
        "This account has used its allowance for this window."
    );
    assert_eq!(
        window_of(env, &author.uid, STORAGE_WRITE).await["used"],
        json!(1),
        "a refused batch spends nothing"
    );
}
