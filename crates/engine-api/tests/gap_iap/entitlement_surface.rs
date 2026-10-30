use super::*;

/// What `GET /v2/iap/subscription/active` says about a row as its clock moves:
/// an entitlement that has to be re-checked with the store stays an entitlement,
/// a lapsed one stops being one, and a refund stops it outright.
pub async fn entitlement_surface(env: &Harness, _console: &str) {
    eprintln!("gap: subscription entitlement reads");
    let player = device_player(&env.client, &env.base, "entitle").await;

    expect(
        env.client
            .get(format!("{}/v2/iap/subscription/active", env.base))
            .send()
            .await
            .unwrap(),
        StatusCode::UNAUTHORIZED,
        "entitlements need a player token",
    )
    .await;

    let empty = entitlements(env, &player.token).await;
    assert_eq!(empty["success"], json!(true), "{empty}");
    assert_eq!(empty["active"], json!(false), "{empty}");
    assert_eq!(empty["products"], json!([]), "{empty}");

    record_subscription(env, &player.token, VAULT, Some(days_ahead(30)), "active").await;
    let held = entitlements(env, &player.token).await;
    assert_eq!(held["active"], json!(true), "{held}");
    assert_eq!(held["products"], json!([VAULT]), "{held}");
    let row = row_for(&held, VAULT);
    assert_eq!(row["state"], json!(1), "a future expiry is active: {row}");
    assert_eq!(
        row_for(&held, VAULT)["needs_revalidation"],
        json!(false),
        "a row the store just confirmed is trusted: {held}"
    );

    // Inside the grace window the entitlement holds, but the state says the
    // store has not confirmed a renewal.
    stamp_subscription(env, &player.uid, Some(hours_ago(6)), None, None).await;
    let graced = entitlements(env, &player.token).await;
    assert_eq!(graced["active"], json!(true), "{graced}");
    assert_eq!(row_for(&graced, VAULT)["state"], json!(2), "{graced}");

    // Past the window it is spent.
    stamp_subscription(env, &player.uid, Some(hours_ago(72)), None, None).await;
    let lapsed = entitlements(env, &player.token).await;
    assert_eq!(lapsed["active"], json!(false), "{lapsed}");
    assert_eq!(lapsed["products"], json!([]), "{lapsed}");
    assert_eq!(row_for(&lapsed, VAULT)["state"], json!(3), "{lapsed}");

    // A renewal the store reports brings it back.
    stamp_subscription(env, &player.uid, Some(days_ahead(30)), None, None).await;
    let renewed = entitlements(env, &player.token).await;
    assert_eq!(row_for(&renewed, VAULT)["state"], json!(1), "{renewed}");

    // A stale confirmation is flagged for revalidation without losing access.
    stamp_subscription(
        env,
        &player.uid,
        Some(days_ahead(30)),
        None,
        Some(hours_ago(24 * 30)),
    )
    .await;
    let stale = entitlements(env, &player.token).await;
    assert_eq!(row_for(&stale, VAULT)["state"], json!(1), "{stale}");
    assert_eq!(
        row_for(&stale, VAULT)["needs_revalidation"],
        json!(true),
        "a row nobody has asked the store about is flagged: {stale}"
    );

    // A refund closes the row however far its expiry reaches.
    stamp_subscription(env, &player.uid, None, Some(Utc::now()), None).await;
    let refunded = entitlements(env, &player.token).await;
    assert_eq!(refunded["active"], json!(false), "{refunded}");
    assert_eq!(row_for(&refunded, VAULT)["state"], json!(0), "{refunded}");

    // The listed subscriptions answer with the same states the gate uses.
    let listed = json_body(
        env.client
            .get(format!("{}/v2/iap/subscription?limit=10", env.base))
            .bearer_auth(&player.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "list subscriptions",
    )
    .await;
    let shown = listed["subscriptions"]
        .as_array()
        .expect("an array")
        .iter()
        .find(|row| row["product_id"] == json!(VAULT))
        .cloned()
        .unwrap_or_else(|| panic!("the refunded row is still listed: {listed}"));
    assert_eq!(shown["state"], json!(0), "{shown}");
}
