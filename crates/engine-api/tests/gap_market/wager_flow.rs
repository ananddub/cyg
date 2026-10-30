use super::*;

pub async fn wager_flow(env: &Harness, console: &str) {
    eprintln!("gap: wagers hold stakes and pay them out once");
    let winner = device_player(&env.client, &env.base, "bettor").await;
    let second = device_player(&env.client, &env.base, "bettor2").await;
    let third = device_player(&env.client, &env.base, "bettor3").await;
    let broke = device_player(&env.client, &env.base, "bettor4").await;
    for player in [&winner, &second, &third] {
        fund(env, console, &player.uid, 50.0).await;
    }
    let room = format!("arena-{}", letters_suffix());

    refusal(
        env.client
            .post(format!("{}/v2/wager/create", env.base))
            .bearer_auth(&winner.token)
            .json(&json!({
                "room": room,
                "players": [winner.uid, second.uid],
                "currency": "gold",
                "amount_each": 20.0,
            }))
            .send()
            .await
            .unwrap(),
        StatusCode::UNAUTHORIZED,
        "opening a wager with a player token",
    )
    .await;

    for (body, what) in [
        (
            json!({"room": room, "players": [winner.uid], "currency": "gold", "amount_each": 20.0}),
            "a wager with one bettor",
        ),
        (
            json!({"room": room, "players": [winner.uid, winner.uid], "currency": "gold", "amount_each": 20.0}),
            "a wager naming a bettor twice",
        ),
        (
            json!({"room": room, "players": [winner.uid, second.uid], "currency": "gold", "amount_each": 0.0}),
            "a wager with no stake",
        ),
        (
            json!({"room": room, "players": [winner.uid, second.uid], "currency": "gold", "amount_each": 2e9}),
            "a wager above the stake limit",
        ),
        (
            json!({"room": "", "players": [winner.uid, second.uid], "currency": "gold", "amount_each": 20.0}),
            "a wager with no room",
        ),
    ] {
        refusal(wager(env, body).await, StatusCode::BAD_REQUEST, what).await;
    }
    for player in [&winner, &second, &third] {
        assert_eq!(
            gold_of(env, console, &player.uid).await,
            50.0,
            "a refused wager debited nobody"
        );
    }

    // The account that cannot cover its stake fails the whole room, and the
    // stakes already taken have to be back in their owners' pockets.
    refusal(
        wager(
            env,
            json!({
                "room": room,
                "players": [winner.uid, second.uid, broke.uid],
                "currency": "gold",
                "amount_each": 20.0,
            }),
        )
        .await,
        StatusCode::BAD_REQUEST,
        "a wager that includes an account that cannot pay",
    )
    .await;
    assert_eq!(gold_of(env, console, &winner.uid).await, 50.0);
    assert_eq!(gold_of(env, console, &second.uid).await, 50.0);
    assert_eq!(gold_of(env, console, &broke.uid).await, 0.0);
    assert!(
        rows_matching(env, "wager", &room).await.is_empty(),
        "the refused wager wrote no room row"
    );

    let held = json_body(
        wager(
            env,
            json!({
                "room": room,
                "players": [winner.uid, second.uid, third.uid],
                "currency": "gold",
                "amount_each": 20.0,
            }),
        )
        .await,
        StatusCode::OK,
        "open a three-way wager",
    )
    .await;
    let wager_id = held["wager"]["wager_id"]
        .as_str()
        .expect("a wager id")
        .to_owned();
    assert_eq!(held["wager"]["state"], json!("held"));
    for player in [&winner, &second, &third] {
        assert_eq!(
            gold_of(env, console, &player.uid).await,
            30.0,
            "every bettor paid exactly the stake"
        );
    }

    refusal(
        settle_wager(
            env,
            &wager_id,
            json!({"winner": broke.uid, "data": {"reason": "not a bettor"}}),
        )
        .await,
        StatusCode::BAD_REQUEST,
        "settling on someone outside the room",
    )
    .await;
    refusal(
        env.client
            .post(format!("{}/v2/wager/{wager_id}/settle", env.base))
            .bearer_auth(&winner.token)
            .json(&json!({"winner": winner.uid}))
            .send()
            .await
            .unwrap(),
        StatusCode::UNAUTHORIZED,
        "settling with a player token",
    )
    .await;

    let settled = json_body(
        settle_wager(
            env,
            &wager_id,
            json!({"winner": winner.uid, "data": {"scores": [7, 2, 1]}}),
        )
        .await,
        StatusCode::OK,
        "settle the wager on its winner",
    )
    .await;
    assert_eq!(settled["wager"]["state"], json!("settled"));
    assert_eq!(settled["wager"]["outcome"]["scores"], json!([7, 2, 1]));
    assert_eq!(gold_of(env, console, &winner.uid).await, 90.0);
    assert_eq!(gold_of(env, console, &second.uid).await, 30.0);
    assert_eq!(gold_of(env, console, &third.uid).await, 30.0);

    refusal(
        settle_wager(env, &wager_id, json!({"winner": second.uid})).await,
        StatusCode::BAD_REQUEST,
        "settling a wager twice",
    )
    .await;
    refusal(
        refund_wager(env, &wager_id).await,
        StatusCode::BAD_REQUEST,
        "refunding a settled wager",
    )
    .await;
    assert_eq!(wager_row(env, &wager_id).await["state"], json!("settled"));

    // A room that never resolves gives every stake back, unchanged.
    let abandoned = json_body(
        wager(
            env,
            json!({
                "room": format!("{}-refund", room),
                "players": [second.uid, third.uid],
                "currency": "gold",
                "amount_each": 25.0,
            }),
        )
        .await,
        StatusCode::OK,
        "open a wager that will be refunded",
    )
    .await;
    let abandoned_id = abandoned["wager"]["wager_id"]
        .as_str()
        .expect("a wager id")
        .to_owned();
    assert_eq!(gold_of(env, console, &second.uid).await, 5.0);
    assert_eq!(gold_of(env, console, &third.uid).await, 5.0);

    let refunded = json_body(
        refund_wager(env, &abandoned_id).await,
        StatusCode::OK,
        "refund the abandoned wager",
    )
    .await;
    assert_eq!(refunded["wager"]["state"], json!("refunded"));
    assert_eq!(gold_of(env, console, &second.uid).await, 30.0);
    assert_eq!(gold_of(env, console, &third.uid).await, 30.0);

    refusal(
        refund_wager(env, &abandoned_id).await,
        StatusCode::BAD_REQUEST,
        "refunding a wager twice",
    )
    .await;
    assert_eq!(gold_of(env, console, &second.uid).await, 30.0);

    refusal(
        settle_wager(env, &uid().to_string(), json!({"winner": winner.uid})).await,
        StatusCode::NOT_FOUND,
        "settling a wager that does not exist",
    )
    .await;
}
