use serde_json::json;
use uuid::Uuid;

use super::*;
use crate::repositories::iap::InsertWalletLedgerParams;
use crate::tests::games::{db, skipped_without_database};

#[tokio::test]
async fn an_account_update_and_custom_id_read_back() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let id = Uuid::new_v4();
    q.create_user_bare(id, "account-user")
        .await
        .expect("the user registers");

    q.update_account(&UpdateAccountParams {
        display_name: Some("Nova".into()),
        avatar_url: None,
        lang_tag: Some("en".into()),
        location: Some("Mars".into()),
        timezone: None,
        metadata: json!({"rank": 7}),
        username: "account-user".into(),
        id,
    })
    .await
    .expect("the profile updates");
    let user = q.get_user_by_id(id).await.expect("the user reads back");
    assert_eq!(user.display_name.as_deref(), Some("Nova"));
    assert_eq!(user.metadata["rank"], 7);

    q.set_custom_id(id, "nova-1")
        .await
        .expect("the custom id sets");
    let found = q
        .get_user_by_custom_id("nova-1")
        .await
        .expect("the custom id resolves");
    assert_eq!(found.id, id);
}

#[tokio::test]
async fn a_wallet_ledger_entry_deletes_once() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let user = Uuid::new_v4();
    q.create_user_bare(user, "ledger-user")
        .await
        .expect("the user registers");
    let ledger = Uuid::new_v4();
    q.insert_wallet_ledger(&InsertWalletLedgerParams {
        id: ledger,
        user_id: user,
        changeset: json!({"coins": 5}),
        metadata: json!({}),
    })
    .await
    .expect("the ledger entry appends");

    assert!(
        q.delete_wallet_ledger(ledger, user)
            .await
            .expect("the first delete removes")
    );
    assert!(
        !q.delete_wallet_ledger(ledger, user)
            .await
            .expect("the second delete is a no-op")
    );
}
