use super::*;

const REFUSAL: &str = "This quest is part of a subscription track that is not active.";

/// A quest that names a store product is only offered to, and only claimable by,
/// an account whose subscription to that product is paying out.
pub async fn quest_entitlement(env: &Harness, console: &str) {
    eprintln!("gap: a subscription track gates its quests");

    for product in ["", &"p".repeat(129)] {
        assert_eq!(
            refusal(
                env.client
                    .post(format!("{}/v2/console/quests", env.base))
                    .bearer_auth(console)
                    .json(&json!({
                        "quest_key": format!("bad-{}", letters_suffix()),
                        "title": "A gated quest",
                        "goal_kind": "wins",
                        "goal_amount": 1,
                        "requires_product": product,
                        "reward": { "currency": { "gold": 7 } },
                    }))
                    .send()
                    .await
                    .unwrap(),
                StatusCode::BAD_REQUEST,
                "gate a quest on a product name the operator cannot mean",
            )
            .await,
            "A quest's required product is 1-128 characters."
        );
    }

    let open = format!("open-{}", letters_suffix());
    let vault = format!("vault-{}", letters_suffix());
    create_quest(
        env,
        console,
        json!({
            "quest_key": open,
            "title": "Open to everyone",
            "goal_kind": "wins",
            "goal_amount": 1,
            "reward": { "currency": { "gold": 3 } },
        }),
    )
    .await;
    create_quest(
        env,
        console,
        json!({
            "quest_key": vault,
            "title": "Behind the vault pass",
            "goal_kind": "wins",
            "goal_amount": 1,
            "requires_product": VAULT,
            "reward": { "currency": { "gold": 7 } },
        }),
    )
    .await;

    let none = device_player(&env.client, &env.base, "nofsub").await;
    let holder = device_player(&env.client, &env.base, "holder").await;
    let graced = device_player(&env.client, &env.base, "graced").await;
    let lapsed = device_player(&env.client, &env.base, "lapsed").await;
    let refund = device_player(&env.client, &env.base, "refunded").await;
    let players = [&none, &holder, &graced, &lapsed, &refund];
    ingest(
        env,
        json!(
            players
                .iter()
                .map(|player| json!({ "user_id": player.uid, "name": "wins", "value": 1 }))
                .collect::<Vec<_>>()
        ),
    )
    .await;

    record_subscription(env, &holder.token, VAULT, Some(days_ahead(30)), "holder").await;
    record_subscription(env, &graced.token, VAULT, Some(days_ahead(30)), "graced").await;
    stamp_subscription(env, &graced.uid, Some(hours_ago(6)), None, None).await;
    record_subscription(env, &lapsed.token, VAULT, Some(days_ahead(30)), "lapsed").await;
    stamp_subscription(env, &lapsed.uid, Some(hours_ago(72)), None, None).await;
    record_subscription(env, &refund.token, VAULT, Some(days_ahead(30)), "refund").await;
    stamp_subscription(env, &refund.uid, None, Some(Utc::now()), None).await;

    // A quest nobody is subscribed to is not offered, and cannot be claimed.
    let unlisted = quests_of(env, &none.token).await;
    assert!(
        lists(&unlisted, &open),
        "an ungated quest is offered to everyone: {unlisted:?}"
    );
    assert!(
        !lists(&unlisted, &vault),
        "a gated quest is hidden from an unsubscribed account: {unlisted:?}"
    );
    assert_eq!(
        refusal(
            claim_quest(env, &none.token, &vault).await,
            StatusCode::BAD_REQUEST,
            "claim a gated quest with no subscription",
        )
        .await,
        REFUSAL
    );
    let free = json_body(
        claim_quest(env, &none.token, &open).await,
        StatusCode::OK,
        "claim an ungated quest with no subscription",
    )
    .await;
    assert_eq!(free["reward"]["currency"]["gold"], json!(3.0), "{free}");

    // Active and grace-window rows both pay out; the row says which it is.
    for (label, player) in [("active", &holder), ("grace", &graced)] {
        let listed = quests_of(env, &player.token).await;
        let row = listed
            .iter()
            .find(|quest| quest["quest_key"] == json!(vault.as_str()))
            .unwrap_or_else(|| panic!("a {label} subscriber is not offered the quest: {listed:?}"));
        assert_eq!(row["requires_product"], json!(VAULT), "{row}");
        let claimed = json_body(
            claim_quest(env, &player.token, &vault).await,
            StatusCode::OK,
            &format!("claim the gated quest while {label}"),
        )
        .await;
        assert_eq!(
            claimed["reward"]["currency"]["gold"],
            json!(7.0),
            "{claimed}"
        );
    }

    // Expired and refunded rows are refused, even with the goal already met.
    for (label, player) in [("expired", &lapsed), ("refunded", &refund)] {
        let listed = quests_of(env, &player.token).await;
        assert!(
            !lists(&listed, &vault),
            "a {label} subscription hides its quest: {listed:?}"
        );
        assert_eq!(
            refusal(
                claim_quest(env, &player.token, &vault).await,
                StatusCode::BAD_REQUEST,
                &format!("claim the gated quest after it {label}"),
            )
            .await,
            REFUSAL
        );
    }

    // Renewing the row reopens the track for a player who was locked out.
    stamp_subscription(env, &lapsed.uid, Some(days_ahead(30)), None, None).await;
    let reopened = json_body(
        claim_quest(env, &lapsed.token, &vault).await,
        StatusCode::OK,
        "claim the gated quest once the subscription renews",
    )
    .await;
    assert_eq!(reopened["reward"]["currency"]["gold"], json!(7.0));
}
