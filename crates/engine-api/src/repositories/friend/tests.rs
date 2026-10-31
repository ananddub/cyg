use uuid::Uuid;

use super::*;
use crate::tests::games::{db, skipped_without_database};

#[tokio::test]
async fn friend_edges_store_state_and_delete_both_directions() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let alice = Uuid::new_v4();
    let bob = Uuid::new_v4();
    q.create_user_bare(alice, "alice")
        .await
        .expect("alice registers");
    q.create_user_bare(bob, "bob").await.expect("bob registers");

    q.add_friend_edge(&AddFriendEdgeParams {
        source_id: alice,
        destination_id: bob,
        state: 0,
        position: 0,
    })
    .await
    .expect("the invite edge stores");
    assert_eq!(
        q.get_friend_edge_state(alice, bob)
            .await
            .expect("the edge reads"),
        0
    );

    q.add_friend_edge(&AddFriendEdgeParams {
        source_id: bob,
        destination_id: alice,
        state: 3,
        position: 0,
    })
    .await
    .expect("the accepted edge stores");
    assert_eq!(
        q.get_friend_edge_state(bob, alice)
            .await
            .expect("the edge reads"),
        3
    );

    q.delete_friend_edges(alice, bob)
        .await
        .expect("both directions delete");
    assert!(q.get_friend_edge_state(alice, bob).await.is_err());
    assert!(q.get_friend_edge_state(bob, alice).await.is_err());
}
