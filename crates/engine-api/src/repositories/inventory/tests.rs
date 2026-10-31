use serde_json::json;
use uuid::Uuid;

use super::*;
use crate::tests::games::{db, skipped_without_database};

#[tokio::test]
async fn an_item_definition_upserts_and_reads_back() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");

    q.upsert_item_definition("potion", "Potion", "consumable", 99, "common", &json!({"heal": 5}))
        .await
        .expect("a definition registers");
    let row = q
        .get_item_definition("potion")
        .await
        .expect("the definition is readable")
        .expect("the definition exists");
    assert_eq!(row.name, "Potion");
    assert_eq!(row.max_stack, 99);

    q.upsert_item_definition("potion", "Elixir", "consumable", 20, "rare", &json!({}))
        .await
        .expect("a second upsert replaces");
    let row = q
        .get_item_definition("potion")
        .await
        .expect("the definition is readable")
        .expect("the definition exists");
    assert_eq!(row.name, "Elixir");
    assert_eq!(row.max_stack, 20);
    assert_eq!(row.rarity, "rare");
}

#[tokio::test]
async fn granted_items_list_by_slot_and_shrink_on_consume() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let user = Uuid::new_v4();
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();

    q.grant_inventory_item(first, user, "sword", 1, 2, &json!({}))
        .await
        .expect("the first item grants");
    q.grant_inventory_item(second, user, "potion", 5, 0, &json!({}))
        .await
        .expect("the second item grants");

    let items = q.list_inventory(user).await.expect("the bag lists");
    let ids: Vec<Uuid> = items.iter().map(|row| row.instance_id).collect();
    assert_eq!(ids, vec![second, first], "rows order by slot index");

    q.update_inventory_quantity(second, 2)
        .await
        .expect("the stack shrinks");
    let row = q
        .get_inventory_item(second)
        .await
        .expect("the item is readable")
        .expect("the item exists");
    assert_eq!(row.quantity, 2);

    q.update_inventory_quantity(second, 0)
        .await
        .expect("an empty stack is removed");
    assert!(
        q.get_inventory_item(second)
            .await
            .expect("the lookup runs")
            .is_none()
    );
}

#[tokio::test]
async fn equipping_moves_the_item_to_the_named_slot() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let id = Uuid::new_v4();
    q.grant_inventory_item(id, Uuid::new_v4(), "sword", 1, 0, &json!({}))
        .await
        .expect("the item grants");

    q.set_inventory_slot(id, 7).await.expect("the slot moves");
    let row = q
        .get_inventory_item(id)
        .await
        .expect("the item is readable")
        .expect("the item exists");
    assert_eq!(row.slot_index, 7);
}

#[tokio::test]
async fn a_room_snapshot_saves_and_loads() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let room = format!("room-{}", Uuid::new_v4());

    assert!(
        q.load_room_snapshot(&room)
            .await
            .expect("the lookup runs")
            .is_none()
    );
    q.save_room_snapshot(&room, "tank-3d", b"snapshot", 12)
        .await
        .expect("the snapshot saves");

    let row = q
        .load_room_snapshot(&room)
        .await
        .expect("the snapshot is readable")
        .expect("the snapshot exists");
    assert_eq!(row.backend, "tank-3d");
    assert_eq!(row.entity_count, 12);
    assert_eq!(row.snapshot_bytes, b"snapshot");
}
