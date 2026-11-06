use super::*;

/// A tournament that names a published rule gates both join and write on the
/// player's current stat map; a tournament naming an unpublished rule fails
/// closed instead of silently opening up.
pub async fn tournament_rules(env: &Harness, console: &str) {
    eprintln!("gap: tournament rule gating on join and score");
    let suffix = letters_suffix();
    let rule_key = format!("e2e-trule-{suffix}");
    let gate_id = format!("e2e_gap_trule_{suffix}");
    let unpublished_id = format!("e2e_gap_tmissing_{suffix}");
    let now = Utc::now();

    let draft = json_body(
        env.client
            .post(format!("{}/v2/console/rules", env.base))
            .bearer_auth(console)
            .json(&json!({
                "key": rule_key,
                "body": { "field": "kills", "op": "gte", "value": 10 },
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "drafting the tournament rule",
    )
    .await;
    assert_eq!(draft["version"], 1);
    json_body(
        env.client
            .post(format!(
                "{}/v2/console/rules/{rule_key}/publish/1",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "publishing the tournament rule",
    )
    .await;

    let created = json_body(
        env.client
            .post(format!("{}/v2/console/tournament", env.base))
            .bearer_auth(console)
            .json(&json!({
                "id": gate_id,
                "title": "Rule Gated Round",
                "duration": 3_600,
                "start_time": (now - ChronoDuration::seconds(300)).to_rfc3339(),
                "join_required": true,
                "rule_key": rule_key,
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "creating a rule-gated tournament",
    )
    .await;
    assert_eq!(created["rule_key"], json!(rule_key));

    let kept = json_body(
        env.client
            .put(format!("{}/v2/console/tournament/{gate_id}", env.base))
            .bearer_auth(console)
            .json(&json!({ "title": "Rule Gated Round v2" }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "updating a tournament without naming its rule",
    )
    .await;
    assert_eq!(
        kept["rule_key"],
        json!(rule_key),
        "an omitted rule key keeps the stored rule"
    );

    let cleared = json_body(
        env.client
            .put(format!("{}/v2/console/tournament/{gate_id}", env.base))
            .bearer_auth(console)
            .json(&json!({ "rule_key": "" }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "clearing the rule",
    )
    .await;
    assert!(
        cleared.get("rule_key").is_none(),
        "an empty key clears the rule: {cleared}"
    );

    // Put the rule back for the enforcement assertions.
    json_body(
        env.client
            .put(format!("{}/v2/console/tournament/{gate_id}", env.base))
            .bearer_auth(console)
            .json(&json!({ "rule_key": rule_key }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "restoring the rule",
    )
    .await;

    let player = device_player(&env.client, &env.base, "rule").await;
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/tournament/{gate_id}/join", env.base))
                .bearer_auth(&player.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "joining below the rule threshold",
        )
        .await,
        "Player does not meet the tournament rule."
    );

    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/tournament/{gate_id}", env.base))
                .bearer_auth(&player.token)
                .json(&json!({ "score": 50 }))
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "scoring below the rule threshold",
        )
        .await,
        "Player does not meet the tournament rule."
    );

    json_body(
        env.client
            .post(format!("{}/v2/events", env.base))
            .header(reqwest::header::AUTHORIZATION, server_key_header())
            .json(&json!({ "events": [{ "user_id": player.uid, "name": "kills", "value": 10 }] }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "crediting the stat the rule reads",
    )
    .await;

    json_body(
        env.client
            .post(format!("{}/v2/tournament/{gate_id}/join", env.base))
            .bearer_auth(&player.token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "joining at the rule threshold",
    )
    .await;
    let scored = json_body(
        env.client
            .post(format!("{}/v2/tournament/{gate_id}", env.base))
            .bearer_auth(&player.token)
            .json(&json!({ "score": 50 }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "scoring at the rule threshold",
    )
    .await;
    assert_eq!(scored["score"], "50");

    json_body(
        env.client
            .post(format!("{}/v2/console/tournament", env.base))
            .bearer_auth(console)
            .json(&json!({
                "id": unpublished_id,
                "title": "Missing Rule Round",
                "duration": 3_600,
                "start_time": (now - ChronoDuration::seconds(300)).to_rfc3339(),
                "join_required": true,
                "rule_key": format!("e2e-missing-{suffix}"),
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "creating a tournament with an unpublished rule",
    )
    .await;
    assert_eq!(
        refusal(
            env.client
                .post(format!("{}/v2/tournament/{unpublished_id}/join", env.base))
                .bearer_auth(&player.token)
                .send()
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "joining a tournament whose rule was never published",
        )
        .await,
        "Tournament rule is not published."
    );

    json_body(
        env.client
            .delete(format!("{}/v2/console/rules/{rule_key}", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "cleaning up the tournament rule",
    )
    .await;
}
