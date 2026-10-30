use super::*;

/// Badges are a threshold and a counter: nothing is granted, nothing is stored
/// against the player, and re-tuning a threshold re-judges everybody at once.
pub async fn badge_surface(env: &Harness, console: &str, host: &Player) {
    let manage = format!("{}/v2/console/badges", env.base);
    let mine = format!("{}/v2/badges", env.base);

    refusal(
        env.client.get(&manage).send().await.unwrap(),
        StatusCode::UNAUTHORIZED,
        "the badge catalogue",
    )
    .await;
    refusal(
        env.client.get(&mine).send().await.unwrap(),
        StatusCode::UNAUTHORIZED,
        "a player's badges",
    )
    .await;

    assert!(
        player_badges(env, host).await.is_empty(),
        "nothing is defined yet"
    );

    for (body, why) in [
        (
            json!({ "badge_key": "", "title": "No key", "stat": "wins", "threshold": 1 }),
            "a badge with no key",
        ),
        (
            json!({ "badge_key": "Bad Key", "title": "Uppercase", "stat": "wins", "threshold": 1 }),
            "a key with a space in it",
        ),
        (
            json!({ "badge_key": "ok", "title": "  ", "stat": "wins", "threshold": 1 }),
            "a badge with no title",
        ),
        (
            json!({ "badge_key": "ok", "title": "Bad stat", "stat": "Wins!", "threshold": 1 }),
            "a badge naming something no counter is called",
        ),
        (
            json!({ "badge_key": "ok", "title": "Zero", "stat": "wins", "threshold": 0 }),
            "a badge nobody can fail",
        ),
    ] {
        let refused = env
            .client
            .post(&manage)
            .bearer_auth(console)
            .json(&body)
            .send()
            .await
            .unwrap();
        refusal(refused, StatusCode::BAD_REQUEST, why).await;
    }

    let won = post_badge(env, console, "won-twice", "wins", 2, None).await;
    assert_eq!(won["badge_key"], json!("won-twice"));
    assert_eq!(won["is_active"], true, "a new badge is live");

    let cast = post_badge(env, console, "first-cast", "casts", 1, None).await;
    assert_eq!(cast["stat"], json!("casts"));

    let clash = env
        .client
        .post(&manage)
        .bearer_auth(console)
        .json(&json!({
            "badge_key": "won-twice", "title": "Again", "stat": "wins", "threshold": 9,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        refusal(clash, StatusCode::CONFLICT, "a key already taken").await,
        "A badge with this key already exists.",
        "two badges cannot answer to one key"
    );

    let earned = held_badges(env, host).await;
    badge_row(&earned, "won-twice", 2, true);
    badge_row(&earned, "first-cast", 0, false);

    // The retired badge stays in the catalogue for the console and disappears
    // from the player's shelf.
    post_badge(env, console, "retired", "wins", 1, Some(false)).await;
    assert!(
        player_badges(env, host)
            .await
            .iter()
            .all(|row| row["badge_key"] != json!("retired")),
        "an inactive badge is not offered"
    );
    assert!(
        console_badges(env, console)
            .await
            .iter()
            .any(|row| row["badge_key"] == json!("retired")),
        "but it is still an operator's row"
    );

    // Re-tuning the threshold re-judges the same counter the other way.
    put_badge(env, console, "won-twice", "wins", 3, Some(true)).await;
    badge_row(&held_badges(env, host).await, "won-twice", 2, false);

    let gone = env
        .client
        .delete(format!("{manage}/won-twice"))
        .bearer_auth(console)
        .send()
        .await
        .unwrap();
    expect(gone, StatusCode::OK, "delete a badge").await;
    assert!(
        console_badges(env, console)
            .await
            .iter()
            .all(|row| row["badge_key"] != json!("won-twice")),
        "the definition is gone"
    );
    assert!(
        player_badges(env, host)
            .await
            .iter()
            .all(|row| row["badge_key"] != json!("won-twice")),
        "and the player no longer sees it"
    );

    let missing = env
        .client
        .delete(format!("{manage}/won-twice"))
        .bearer_auth(console)
        .send()
        .await
        .unwrap();
    assert_eq!(
        refusal(missing, StatusCode::NOT_FOUND, "a badge already deleted").await,
        "Badge not found.",
        "deleting twice says so"
    );
}

async fn post_badge(
    env: &Harness,
    console: &str,
    key: &str,
    stat: &str,
    threshold: i64,
    is_active: Option<bool>,
) -> Value {
    json_body(
        env.client
            .post(format!("{}/v2/console/badges", env.base))
            .bearer_auth(console)
            .json(&badge_body(key, stat, threshold, is_active))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "create a badge",
    )
    .await
}

async fn put_badge(
    env: &Harness,
    console: &str,
    key: &str,
    stat: &str,
    threshold: i64,
    is_active: Option<bool>,
) -> Value {
    json_body(
        env.client
            .put(format!("{}/v2/console/badges/{key}", env.base))
            .bearer_auth(console)
            .json(&badge_body(key, stat, threshold, is_active))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "write a badge",
    )
    .await
}

fn badge_body(key: &str, stat: &str, threshold: i64, is_active: Option<bool>) -> Value {
    json!({
        "badge_key": key,
        "title": format!("{key} badge"),
        "detail": format!("Reach {threshold} {stat}."),
        "stat": stat,
        "threshold": threshold,
        "is_active": is_active,
    })
}

async fn console_badges(env: &Harness, console: &str) -> Vec<Value> {
    let listed = json_body(
        env.client
            .get(format!("{}/v2/console/badges", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "list the catalogue",
    )
    .await;
    listed
        .as_array()
        .expect("a list of badge definitions")
        .clone()
}

async fn player_badges(env: &Harness, host: &Player) -> Vec<Value> {
    let listed = json_body(
        env.client
            .get(format!("{}/v2/badges", env.base))
            .bearer_auth(&host.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "list the player's badges",
    )
    .await;
    listed["badges"].as_array().expect("a badge list").clone()
}

/// The rows a player holds, and the two facts each one states: the counter's
/// value and whether that reaches the threshold.
async fn held_badges(env: &Harness, host: &Player) -> Vec<Value> {
    let rows = player_badges(env, host).await;
    assert!(!rows.is_empty(), "the player is shown something");
    rows
}

fn badge_row(rows: &[Value], key: &str, value: i64, earned: bool) {
    let row = rows
        .iter()
        .find(|row| row["badge_key"] == json!(key))
        .unwrap_or_else(|| panic!("{key} is on the shelf: {rows:?}"));
    assert_eq!(
        (row["value"].as_i64(), row["earned"].as_bool()),
        (Some(value), Some(earned)),
        "{key} reads the counter it names"
    );
}
