use super::*;

#[tokio::test]
pub(crate) async fn upsert_replaces_on_conflict() {
    let db = memory().await;
    db.apply([
        ProbeRow::define_table(),
        "DEFINE INDEX IF NOT EXISTS probe_row_owner2 ON probe_row FIELDS owner UNIQUE;",
    ])
    .await
    .expect("schema");

    let at = chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 10, 29, 12, 0, 0).unwrap();
    let owner = uuid::Uuid::new_v4();
    for title_at in [at, at + chrono::Duration::seconds(1)] {
        let id = uuid::Uuid::new_v4();
        let params = BTreeMap::from([
            ("owner".to_string(), serde_json::json!(owner.to_string())),
            ("id".to_string(), serde_json::json!(id.to_string())),
            (
                "data".to_string(),
                serde_json::json!({ "owner": owner.to_string(), "at": time::encode(title_at) }),
            ),
        ]);
        db.exec_params(
            "DELETE probe_row WHERE owner = $owner; \
             CREATE type::record('probe_row', <uuid>$id) CONTENT $data RETURN AFTER;",
            &params,
        )
        .await
        .expect("replace");
    }
    let rows: Vec<ProbeRow> = db
        .query_params(
            "SELECT * FROM probe_row WHERE owner = $owner",
            &BTreeMap::from([("owner".to_string(), serde_json::json!(owner.to_string()))]),
        )
        .await
        .expect("select");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].at, at + chrono::Duration::seconds(1));
}
#[tokio::test]
pub(crate) async fn later_statement_errors_are_not_lost() {
    let db = memory().await;
    db.exec("DEFINE TABLE strict SCHEMAFULL;")
        .await
        .expect("define strict table");
    let err = db
        .exec("RETURN 1; CREATE strict SET nope = true;")
        .await
        .expect_err("statement 1 must fail the call");
    eprintln!("later error: {err}");
}
