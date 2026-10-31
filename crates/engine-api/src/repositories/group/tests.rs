use serde_json::json;
use uuid::Uuid;

use super::*;
use crate::tests::games::{db, skipped_without_database};

#[tokio::test]
async fn a_group_registers_reads_back_and_changes_state() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let id = Uuid::new_v4();
    let creator = Uuid::new_v4();
    q.create_group(&CreateGroupParams {
        id,
        creator_id: creator,
        name: "Gap Squad".into(),
        description: "a group".into(),
        avatar_url: String::new(),
        lang_tag: "en".into(),
        metadata: json!({}),
        max_count: 10,
        state: 0,
    })
    .await
    .expect("the group registers");

    assert_eq!(q.get_group_by_id(id).await.expect("by id reads").id, id);
    assert_eq!(
        q.get_group_by_name("Gap Squad")
            .await
            .expect("by name reads")
            .id,
        id
    );

    q.set_group_state(id, 1).await.expect("the state moves");
    let row = q.get_group_by_id(id).await.expect("the row reads");
    assert_eq!(row.state, 1);
}

#[tokio::test]
async fn a_group_edge_carries_its_state() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let group = Uuid::new_v4();
    let user = Uuid::new_v4();
    q.create_group(&CreateGroupParams {
        id: group,
        creator_id: user,
        name: "Edge Squad".into(),
        description: String::new(),
        avatar_url: String::new(),
        lang_tag: "en".into(),
        metadata: json!({}),
        max_count: 10,
        state: 0,
    })
    .await
    .expect("the group registers");

    q.join_group_edge(&AddFriendEdgeParams {
        source_id: group,
        destination_id: user,
        state: 2,
        position: 0,
    })
    .await
    .expect("the edge stores");
    assert_eq!(
        q.get_group_edge_state(group, user)
            .await
            .expect("the state reads"),
        2
    );

    q.leave_group_edge(group, user)
        .await
        .expect("the edge leaves");
    assert!(q.get_group_edge_state(group, user).await.is_err());
}
