use uuid::Uuid;

use super::*;
use crate::tests::games::{db, skipped_without_database};

#[tokio::test]
async fn a_batch_lookup_returns_each_registered_user() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let alice = Uuid::new_v4();
    let bob = Uuid::new_v4();
    q.create_user_bare(alice, "batch-alice")
        .await
        .expect("alice registers");
    q.create_user_bare(bob, "batch-bob")
        .await
        .expect("bob registers");

    let found = q
        .users_by_id(&[alice, bob, alice, Uuid::new_v4()])
        .await
        .expect("the batch reads");
    assert_eq!(found.len(), 2);
    assert_eq!(found[&bob].username, "batch-bob");
}

#[tokio::test]
async fn searching_by_name_matches_the_substring_and_skips_disabled_users() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let id = Uuid::new_v4();
    q.create_user_bare(id, "searchable-user")
        .await
        .expect("the user registers");

    let found = q
        .search_users_by_name("SEARCHABLE", 10)
        .await
        .expect("the search runs");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, id);
    assert_eq!(
        q.get_user_by_username_single("searchable-user")
            .await
            .expect("the exact lookup reads")
            .id,
        id
    );
}
