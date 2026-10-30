use super::*;

pub async fn listing_escrow(env: &Harness, console: &str) {
    eprintln!("gap: a listing holds the item and cancels back out of it");
    let seller = device_player(&env.client, &env.base, "seller").await;
    let stranger = device_player(&env.client, &env.base, "stranger").await;
    let item = format!("blade-{}", letters_suffix());
    define_item(env, console, &item).await;
    let instance = grant_instance(env, &seller.uid, &item).await;
    assert!(
        holds(env, &seller.token, &instance).await.is_some(),
        "the grant put the item in the seller's pack"
    );

    let listed = json_body(
        list_item(env, &seller.token, &instance, 40.0).await,
        StatusCode::OK,
        "put the item up for sale",
    )
    .await;
    assert_eq!(listed["listing"]["state"], json!("open"), "{listed}");
    let listing = listed["listing"]["listing_id"]
        .as_str()
        .expect("a listing id")
        .to_owned();

    assert!(
        holds(env, &seller.token, &instance).await.is_none(),
        "the listed item left the seller's pack for escrow"
    );

    refusal(
        buy_listing(env, &seller.token, &listing).await,
        StatusCode::BAD_REQUEST,
        "the seller buying their own listing",
    )
    .await;

    // The debit is refused before anything moves, so the listing has to stay
    // open and the escrow hold has to survive for the next attempt.
    refusal(
        buy_listing(env, &stranger.token, &listing).await,
        StatusCode::BAD_REQUEST,
        "a buyer without the gold to cover the price",
    )
    .await;
    assert_eq!(
        listing_row(env, &listing).await["state"],
        json!("open"),
        "the refused buy left the listing open"
    );
    assert_eq!(gold_of(env, console, &stranger.uid).await, 0.0);
    assert!(
        ledger_for(env, &format!("buy:{listing}")).await.is_empty(),
        "a refused buy wrote no ledger row"
    );

    refusal(
        cancel_listing(env, &stranger.token, &listing).await,
        StatusCode::FORBIDDEN,
        "someone else cancelling the listing",
    )
    .await;

    let cancelled = json_body(
        cancel_listing(env, &seller.token, &listing).await,
        StatusCode::OK,
        "the seller cancelling their own listing",
    )
    .await;
    assert_eq!(cancelled["listing"]["state"], json!("cancelled"));
    assert!(
        holds(env, &seller.token, &instance).await.is_some(),
        "cancelling returned the held item to the seller"
    );
    assert_eq!(
        listing_row(env, &listing).await["state"],
        json!("cancelled")
    );

    refusal(
        cancel_listing(env, &seller.token, &listing).await,
        StatusCode::BAD_REQUEST,
        "cancelling a listing that is already closed",
    )
    .await;

    refusal(
        cancel_listing(env, &seller.token, "not-a-record").await,
        StatusCode::BAD_REQUEST,
        "a listing id that is not a record id",
    )
    .await;

    refusal(
        buy_listing(env, &stranger.token, &uid().to_string()).await,
        StatusCode::NOT_FOUND,
        "buying a listing that does not exist",
    )
    .await;

    // A listing needs something to hold: an instance nobody owns cannot be
    // put up, and neither can one that is already on escrow.
    refusal(
        list_item(env, &stranger.token, &instance, 10.0).await,
        StatusCode::BAD_REQUEST,
        "listing an item the seller does not hold",
    )
    .await;
    refusal(
        list_item(env, &seller.token, &uid().to_string(), 10.0).await,
        StatusCode::NOT_FOUND,
        "listing an instance that does not exist",
    )
    .await;
    refusal(
        list_item(env, &seller.token, &instance, -5.0).await,
        StatusCode::BAD_REQUEST,
        "listing at a price that is not positive",
    )
    .await;
}
