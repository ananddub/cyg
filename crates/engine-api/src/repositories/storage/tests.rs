use uuid::Uuid;

use super::*;
use crate::tests::games::{db, skipped_without_database};

async fn insert_row(
    queries: &Queries,
    owner: Uuid,
    collection: &str,
    key: &str,
    value: &str,
    read: Option<i32>,
    write: Option<i32>,
) {
    let object = WriteObject {
        collection: collection.to_string(),
        key: key.to_string(),
        value: value.to_string(),
        version: String::new(),
        permission_read: read,
        permission_write: write,
    };
    let mut params =
        BTreeMap::<String, Value>::from([("owner".to_string(), json!(owner.to_string()))]);
    let statement = storage_insert_statement(0, &object, owner, &mut params);
    queries
        .storage_exec(&statement, params)
        .await
        .expect("the object inserts");
}

#[tokio::test]
async fn a_stored_object_reads_back_by_key_and_by_batch() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let owner = Uuid::new_v4();
    insert_row(q, owner, "saves", "auto", r#"{"hp":10}"#, None, None).await;

    let current = q
        .storage_current("saves", "auto", owner)
        .await
        .expect("the current row reads")
        .expect("the row exists");
    assert_eq!(current.version, version_of(r#"{"hp":10}"#));
    assert_eq!(current.read, PERMIT_OWNER);

    let row = q
        .storage_object(owner, "saves", "auto", owner, None)
        .await
        .expect("the object reads")
        .expect("the object exists");
    assert_eq!(row.version, current.version);
    assert_eq!(row.value, r#"{"hp":10}"#);

    let batch = q
        .storage_objects(owner, &[("saves".to_string(), "auto".to_string(), owner)])
        .await
        .expect("the batch reads");
    assert_eq!(batch.len(), 1);
    assert_eq!(batch[0].key, "auto");
}

#[tokio::test]
async fn listings_separate_own_objects_from_public_ones() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let owner = Uuid::new_v4();
    let other = Uuid::new_v4();
    insert_row(q, owner, "saves", "mine", "{}", None, None).await;
    insert_row(q, other, "saves", "theirs", "{}", None, None).await;
    insert_row(q, other, "saves", "shared", "{}", Some(2), None).await;

    let own = q
        .storage_listing(StorageListing::Own, "saves", Some(owner), None, 100)
        .await
        .expect("the owner listing reads");
    let keys: Vec<&str> = own.iter().map(|row| row.key.as_str()).collect();
    assert_eq!(keys, vec!["mine"]);

    let public = q
        .storage_listing(StorageListing::PublicAll, "saves", None, None, 100)
        .await
        .expect("the public listing reads");
    let keys: Vec<&str> = public.iter().map(|row| row.key.as_str()).collect();
    assert_eq!(keys, vec!["shared"]);
}

#[tokio::test]
async fn a_deleted_object_is_no_longer_readable() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let owner = Uuid::new_v4();
    insert_row(q, owner, "saves", "gone", "{}", None, None).await;

    let ids = [ObjectId {
        collection: "saves".to_string(),
        key: "gone".to_string(),
        user_id: String::new(),
        version: String::new(),
    }];
    let deletable = q
        .storage_deletable(owner, &ids)
        .await
        .expect("the lookup runs");
    assert_eq!(deletable.len(), 1);

    q.storage_delete(owner, &ids)
        .await
        .expect("the object deletes");
    assert!(
        q.storage_object(owner, "saves", "gone", owner, None)
            .await
            .expect("the read runs")
            .is_none()
    );
}

#[tokio::test]
async fn an_index_filters_and_sorts_stored_values() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let owner = Uuid::new_v4();
    insert_row(q, owner, "players", "a", r#"{"level":3,"name":"ann"}"#, Some(2), None).await;
    insert_row(q, owner, "players", "b", r#"{"level":9,"name":"bob"}"#, Some(2), None).await;

    q.storage_insert_index(&StorageIndexContent {
        name: "by-level".to_string(),
        collection: "players".to_string(),
        key: String::new(),
        fields: vec!["level".to_string(), "name".to_string()],
        sortable_fields: vec!["level".to_string()],
        max_entries: 100,
        index_only: false,
        user_id: owner,
        create_time: Utc::now(),
    })
    .await
    .expect("the index registers");

    let def = q
        .storage_index("by-level")
        .await
        .expect("the index reads")
        .expect("the index exists");
    let all = q
        .storage_index_page(
            &def,
            &[],
            &[(StorageIndexSort::ValueField("level".to_string()), true)],
            owner,
            101,
            0,
        )
        .await
        .expect("the index page reads");
    let keys: Vec<&str> = all.iter().map(|row| row.key.as_str()).collect();
    assert_eq!(keys, vec!["b", "a"]);

    let filtered = q
        .storage_index_page(
            &def,
            &[StorageIndexTerm::Range {
                field: "level".to_string(),
                lower: Some(StorageIndexBound {
                    text: "5".to_string(),
                    number: Some("5".to_string()),
                    inclusive: true,
                }),
                upper: None,
            }],
            &[],
            owner,
            101,
            0,
        )
        .await
        .expect("the filtered page reads");
    let keys: Vec<&str> = filtered.iter().map(|row| row.key.as_str()).collect();
    assert_eq!(keys, vec!["b"]);

    q.storage_delete_index("by-level")
        .await
        .expect("the index deletes");
    assert!(
        q.storage_index("by-level")
            .await
            .expect("the lookup runs")
            .is_none()
    );
}
