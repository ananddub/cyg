use super::*;

pub async fn buy_flow(env: &Harness, console: &str) {
    eprintln!("gap: a sale moves the item and the gold together");
    let seller = device_player(&env.client, &env.base, "seller").await;
    let buyer = device_player(&env.client, &env.base, "buyer").await;
    let chaser = device_player(&env.client, &env.base, "chaser").await;
    fund(env, console, &buyer.uid, 100.0).await;
    fund(env, console, &chaser.uid, 100.0).await;

    let item = format!("helm-{}", letters_suffix());
    define_item(env, console, &item).await;
    let instance = grant_instance(env, &seller.uid, &item).await;

    let listed = json_body(
        list_item(env, &seller.token, &instance, 40.0).await,
        StatusCode::OK,
        "put the item up for sale",
    )
    .await;
    let listing = listed["listing"]["listing_id"]
        .as_str()
        .expect("a listing id")
        .to_owned();

    let sold = json_body(
        buy_listing(env, &buyer.token, &listing).await,
        StatusCode::OK,
        "buy the listed item",
    )
    .await;
    assert_eq!(sold["listing"]["state"], json!("sold"), "{sold}");

    assert_eq!(gold_of(env, console, &buyer.uid).await, 60.0);
    assert_eq!(gold_of(env, console, &seller.uid).await, 40.0);
    assert!(
        holds(env, &buyer.token, &instance).await.is_some(),
        "the delivered item sits in the buyer's pack"
    );
    assert!(
        holds(env, &seller.token, &instance).await.is_none(),
        "the seller no longer holds what they sold"
    );

    // A second buyer cannot be charged for an item that already changed hands,
    // and the price cannot be collected twice.
    refusal(
        buy_listing(env, &chaser.token, &listing).await,
        StatusCode::BAD_REQUEST,
        "buying a listing that already sold",
    )
    .await;
    assert_eq!(gold_of(env, console, &chaser.uid).await, 100.0);
    assert_eq!(gold_of(env, console, &buyer.uid).await, 60.0);
    let moved = ledger_for(env, &format!("buy:{listing}")).await;
    assert_eq!(
        moved.len(),
        2,
        "exactly one debit and one credit were written: {moved:?}"
    );

    refusal(
        cancel_listing(env, &seller.token, &listing).await,
        StatusCode::BAD_REQUEST,
        "cancelling a listing that already sold",
    )
    .await;

    let rows = listing_row(env, &listing).await;
    assert_eq!(rows["state"], json!("sold"));
    assert_eq!(rows["price"], json!(40.0));
    assert_eq!(rows["price_currency"], json!("gold"));
}
