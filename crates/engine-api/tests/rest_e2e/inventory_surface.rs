use super::*;

pub async fn inventory_surface(client: &Client, base: &str) {
    eprintln!("e2e: inventory");
    let alice = device_player(client, base, "inv-a").await;
    let bob = device_player(client, base, "inv-b").await;
    // Defining items and granting them are privileged (console/server) calls,
    // so they authenticate with the console session rather than a player token.
    let console = console_login(client, base).await;
    let item_id = format!("e2e-item-{}", letters_suffix());
    let inventory = format!("{base}/v2/inventory");

    let empty = api_get(client, inventory.clone(), &alice.token, "empty inventory").await;
    assert_eq!(empty["items"], json!([]));

    let defined = api_post(
        client,
        format!("{inventory}/define"),
        &console,
        json!({
            "item_id": item_id,
            "name": "E2E Sword",
            "category": "weapon",
            "max_stack": 5,
            "rarity": "rare",
            "properties": { "damage": 12 },
        }),
        "define an item",
    )
    .await;
    assert_eq!(defined["success"], true);
    assert_eq!(
        defined["message"],
        format!("item definition '{item_id}' registered")
    );
    assert!(defined["item"].is_null());

    refusal(
        server_grant(
            client,
            &inventory,
            json!({ "user_id": alice.uid, "item_id": format!("{item_id}-missing") }),
        )
        .await,
        StatusCode::NOT_FOUND,
        "grant an undefined item",
    )
    .await;

    let granted = json_body(
        server_grant(client, &inventory, json!({ "user_id": alice.uid, "item_id": item_id, "quantity": 99, "metadata": { "bound": true } })).await,
        StatusCode::OK,
        "grant an item beyond its stack",
    )
    .await;
    assert_eq!(granted["message"], "granted 5x E2E Sword");
    assert_eq!(granted["item"]["quantity"], 5);
    assert_eq!(granted["item"]["slot_index"], -1);
    assert_eq!(granted["item"]["user_id"], alice.uid);
    assert_eq!(granted["item"]["metadata"], json!({ "bound": true }));
    let first_instance = granted["item"]["instance_id"]
        .as_str()
        .expect("instance id")
        .to_owned();

    let zero = json_body(
        server_grant(
            client,
            &inventory,
            json!({ "user_id": alice.uid, "item_id": item_id, "quantity": 0 }),
        )
        .await,
        StatusCode::OK,
        "grant zero of an item",
    )
    .await;
    assert_eq!(zero["item"]["quantity"], 1);
    let second_instance = zero["item"]["instance_id"]
        .as_str()
        .expect("instance id")
        .to_owned();

    let bob_item = json_body(
        server_grant(
            client,
            &inventory,
            json!({ "user_id": bob.uid, "item_id": item_id, "quantity": 2 }),
        )
        .await,
        StatusCode::OK,
        "grant an item to another account",
    )
    .await;
    assert_eq!(bob_item["item"]["user_id"], bob.uid);
    assert_eq!(bob_item["item"]["quantity"], 2);
    let bob_instance = bob_item["item"]["instance_id"]
        .as_str()
        .expect("instance id")
        .to_owned();

    refusal(
        client
            .post(format!("{inventory}/equip"))
            .bearer_auth(&alice.token)
            .json(&json!({ "instance_id": Uuid::new_v4(), "slot_index": 0 }))
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "equip an unknown item",
    )
    .await;
    refusal(
        client
            .post(format!("{inventory}/equip"))
            .bearer_auth(&alice.token)
            .json(&json!({ "instance_id": bob_instance, "slot_index": 0 }))
            .send()
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "equip another account's item",
    )
    .await;
    let equipped = api_post(
        client,
        format!("{inventory}/equip"),
        &alice.token,
        json!({ "instance_id": first_instance, "slot_index": 3 }),
        "equip an own item",
    )
    .await;
    assert_eq!(equipped["success"], true);
    assert_eq!(equipped["message"], "item equipped to slot 3");
    assert_eq!(equipped["item"]["slot_index"], 3);

    refusal(
        client
            .post(format!("{inventory}/consume"))
            .bearer_auth(&alice.token)
            .json(&json!({ "instance_id": first_instance, "quantity": 0 }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "consume nothing",
    )
    .await;
    refusal(
        client
            .post(format!("{inventory}/consume"))
            .bearer_auth(&alice.token)
            .json(&json!({ "instance_id": Uuid::new_v4(), "quantity": 1 }))
            .send()
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "consume an unknown item",
    )
    .await;
    refusal(
        client
            .post(format!("{inventory}/consume"))
            .bearer_auth(&alice.token)
            .json(&json!({ "instance_id": bob_instance, "quantity": 1 }))
            .send()
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "consume another account's item",
    )
    .await;
    refusal(
        client
            .post(format!("{inventory}/consume"))
            .bearer_auth(&alice.token)
            .json(&json!({ "instance_id": first_instance, "quantity": 6 }))
            .send()
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "consume more than the stack holds",
    )
    .await;
    let consumed = api_post(
        client,
        format!("{inventory}/consume"),
        &alice.token,
        json!({ "instance_id": first_instance, "quantity": 2 }),
        "consume part of a stack",
    )
    .await;
    assert_eq!(consumed["message"], "consumed 2 items");
    assert_eq!(consumed["item"]["quantity"], 3);
    let consumed = api_post(
        client,
        format!("{inventory}/consume"),
        &alice.token,
        json!({ "instance_id": first_instance, "quantity": 3 }),
        "consume the rest of a stack",
    )
    .await;
    assert_eq!(consumed["message"], "consumed 3 items");
    assert!(consumed["item"].is_null(), "an emptied stack is removed");

    let listed = api_get(client, inventory.clone(), &alice.token, "alice's inventory").await;
    let items = listed["items"].as_array().expect("items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["instance_id"], second_instance);
    assert_eq!(items[0]["quantity"], 1);

    let bob_listing = api_get(client, inventory, &bob.token, "bob's inventory").await;
    let items = bob_listing["items"].as_array().expect("items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["instance_id"], bob_instance);
}

async fn server_grant(client: &Client, inventory: &str, body: Value) -> reqwest::Response {
    client
        .post(format!("{inventory}/grant"))
        .header(reqwest::header::AUTHORIZATION, server_key_header())
        .json(&body)
        .send()
        .await
        .unwrap()
}
