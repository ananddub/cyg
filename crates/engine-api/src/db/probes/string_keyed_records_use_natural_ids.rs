use super::*;

#[tokio::test]
pub(crate) async fn string_keyed_records_use_natural_ids() {
    let db = memory().await;
    db.apply([NamedProbe::define_table()])
        .await
        .expect("schema");

    let row = NamedProbe {
        id: "weekly".into(),
        title: "Weekly".into(),
    };
    let stored = db
        .create("probe_named", Key::String("weekly".into()), &row)
        .await
        .expect("create");
    assert_eq!(stored.id, "weekly");

    let found: Vec<NamedProbe> = db
        .query_params(
            &format!("SELECT * FROM {}", super::super::record("probe_named", "key")),
            &BTreeMap::from([("key".to_string(), serde_json::json!("weekly"))]),
        )
        .await
        .expect("select by natural key");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].title, "Weekly");
}
#[tokio::test]
pub(crate) async fn stored_timestamps_order_chronologically() {
    let db = memory().await;
    db.apply([ProbeRow::define_table()])
        .await
        .expect("schema");

    let base = chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 10, 29, 12, 0, 0).unwrap();
    for offset in [500, 0] {
        let at = base + chrono::Duration::milliseconds(offset);
        let id = uuid::Uuid::new_v4();
        let row = ProbeRow {
            id,
            owner: uuid::Uuid::new_v4(),
            at,
        };
        db.create("probe_row", Key::Uuid(id), &row)
            .await
            .expect("create");
    }
    let ordered: Vec<ProbeRow> = db
        .query("SELECT * FROM probe_row ORDER BY at ASC")
        .await
        .expect("ordered select");
    assert_eq!(ordered.len(), 2);
    assert!(ordered[0].at < ordered[1].at);
}
#[tokio::test]
pub(crate) async fn unique_index_ignores_missing_values() {
    let db = memory().await;
    db.exec(
        "DEFINE TABLE probe_unique SCHEMALESS;\
         DEFINE INDEX probe_email ON probe_unique FIELDS email UNIQUE;",
    )
    .await
    .expect("define unique table");

    for _ in 0..2 {
        db.exec("CREATE probe_unique SET name = 'no email';")
            .await
            .expect("missing value must not collide");
    }
    db.exec("CREATE probe_unique SET email = 'a@b.c';")
        .await
        .expect("first email");
    let err = db
        .exec("CREATE probe_unique SET email = 'a@b.c';")
        .await
        .expect_err("duplicate email must fail");
    assert!(matches!(err, DbError::Unique(_)), "{err}");
}
#[tokio::test]
pub(crate) async fn update_delete_count_and_cursors_replace_their_sql() {
    let db = memory().await;
    db.apply([NamedProbe::define_table()])
        .await
        .expect("schema");

    let ids: Vec<uuid::Uuid> = (0..3).map(|_| uuid::Uuid::new_v4()).collect();
    for (index, id) in ids.iter().enumerate() {
        let row = NamedProbe {
            id: id.to_string(),
            title: format!("t{index}"),
        };
        db.create("probe_named", Key::Uuid(*id), &row)
            .await
            .expect("create");
    }

    let updated: Vec<NamedProbe> = db
        .query_params(
            "UPDATE probe_named SET title = 'renamed' WHERE title = $title RETURN AFTER",
            &BTreeMap::from([("title".to_string(), serde_json::json!("t1"))]),
        )
        .await
        .expect("update");
    assert_eq!(updated.len(), 1);
    assert_eq!(updated[0].title, "renamed");

    let count: Vec<i64> = db
        .query("RETURN array::len((SELECT id FROM probe_named))")
        .await
        .expect("count");
    assert_eq!(count, vec![3]);

    let deleted: Vec<NamedProbe> = db
        .query_params(
            "DELETE probe_named WHERE title = $title RETURN BEFORE",
            &BTreeMap::from([("title".to_string(), serde_json::json!("t2"))]),
        )
        .await
        .expect("delete");
    assert_eq!(deleted.len(), 1);

    let remaining: Vec<NamedProbe> = db
        .query("SELECT * FROM probe_named ORDER BY title LIMIT 1 START 1")
        .await
        .expect("limit/start");
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].title, "t0");
}
