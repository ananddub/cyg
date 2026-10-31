use super::*;

pub async fn console_leaderboard_record_delete(env: &Harness, console: &str) {
    eprintln!("gap: console leaderboard record deletion");
    let player = device_player(&env.client, &env.base, "board").await;
    let board = format!("e2e-console-lb-{}", letters_suffix());
    seed_leaderboard(env.db(), &board, false).await;
    json_body(
        env.client
            .post(format!("{}/v2/leaderboard/{board}", env.base))
            .bearer_auth(&player.token)
            .json(&json!({"score": 42}))
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "write the player's record",
    )
    .await;
    let rows = count(
        env.db(),
        "leaderboard_record",
        Some("leaderboard_id = $board AND (owner_id = <uuid>$owner OR owner_id = $owner)"),
        &[("board", json!(board)), ("owner", json!(player.uid))],
    )
    .await;
    assert_eq!(rows, 1, "the write is stored");

    refusal(
        env.client
            .delete(format!(
                "{}/v2/console/leaderboard/{board}/owner/not-a-uuid",
                env.base
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "a malformed owner id",
    )
    .await;

    expect(
        env.client
            .delete(format!(
                "{}/v2/console/leaderboard/{board}/owner/{}",
                env.base, player.uid
            ))
            .bearer_auth(console)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "DELETE /v2/console/leaderboard/{id}/owner/{owner_id}",
    )
    .await;
    let rows = count(
        env.db(),
        "leaderboard_record",
        Some("leaderboard_id = $board AND (owner_id = <uuid>$owner OR owner_id = $owner)"),
        &[("board", json!(board)), ("owner", json!(player.uid))],
    )
    .await;
    assert_eq!(rows, 0, "the record is deleted");

    let tournament = format!("e2e-console-tb-{}", letters_suffix());
    seed_leaderboard(env.db(), &tournament, true).await;
    assert_eq!(
        refusal(
            env.client
                .delete(format!(
                    "{}/v2/console/leaderboard/{tournament}/owner/{}",
                    env.base, player.uid
                ))
                .bearer_auth(console)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
            "deleting a record from a tournament board",
        )
        .await,
        "Leaderboard not found."
    );
}
