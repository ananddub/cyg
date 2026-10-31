use super::*;

pub async fn console_match_surface(env: &Harness, console: &str) {
    eprintln!("gap: console match registry");
    let match_id = RealtimeMatchService::create_match_with_tokens(15, vec!["e2e-token".into()]);
    let listed = json_body(
        env.client
            .get(format!("{}/v2/console/match", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/match",
    )
    .await;
    let rows = listed["matches"]
        .as_array()
        .expect("the console list wraps its rows")
        .clone();
    let row = rows
        .iter()
        .find(|r| r["api_match"]["match_id"] == match_id.simple().to_string())
        .unwrap_or_else(|| panic!("the seeded match is listed: {listed}"));
    assert_eq!(row["api_match"]["tick_rate"], 15);
    assert_eq!(row["api_match"]["authoritative"], false);
    assert!(row["node"].is_string(), "every row names its node");

    let state = json_body(
        env.client
            .get(format!("{}/v2/console/match/{match_id}/state", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/match/{id}/state",
    )
    .await;
    assert!(state["presences"].is_array(), "the state names presences");
    assert_eq!(state["tick"], 0, "a relay match does not tick");

    refusal(
        env.client
            .get(format!("{}/v2/console/match/not-a-uuid/state", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "a malformed match id in the state route",
    )
    .await;
    refusal(
        env.client
            .delete(format!("{}/v2/console/match/{}", env.base, Uuid::new_v4()))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "terminating a match that never existed",
    )
    .await;

    expect(
        env.client
            .delete(format!("{}/v2/console/match/{match_id}", env.base))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/console/match/{id}",
    )
    .await;
    assert!(
        !RealtimeMatchService::check_match_exists(match_id),
        "the registry dropped the terminated match"
    );
    let trail = json_body(
        env.client
            .get(format!(
                "{}/v2/console/audit/log?action=match.terminate&limit=20",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "the audit trail for the termination",
    )
    .await;
    assert!(
        trail
            .as_array()
            .map(|rows| rows.as_slice())
            .unwrap_or(&[])
            .iter()
            .any(|row| row["action"] == "match.terminate"),
        "the termination is audited: {trail}"
    );
}
