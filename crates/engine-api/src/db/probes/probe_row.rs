use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("probe_row", schemaless)]
pub(crate) struct ProbeRow {
    #[field(thing)]
    pub(crate) id: uuid::Uuid,
    pub(crate) owner: uuid::Uuid,
    #[serde(with = "time::iso")]
    pub(crate) at: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("probe_named", schemaless)]
pub(crate) struct NamedProbe {
    #[field(thing)]
    pub(crate) id: String,
    pub(crate) title: String,
}
pub(crate) async fn memory() -> Datastore {
    Datastore::open(
        "mem://",
        "probe",
        "probe",
        None,
        None,
        KvBudget::default(),
    )
    .await
    .expect("open mem://")
}
#[tokio::test]
pub(crate) async fn bytes_round_trip() {
    let db = memory().await;
    db.exec("DEFINE TABLE probe SCHEMAFULL; DEFINE FIELD blob ON probe TYPE bytes;")
        .await
        .expect("define bytes field");
    let params = BTreeMap::from([("b".to_string(), serde_json::json!("AQID"))]);
    let rows: Vec<serde_json::Value> = db
        .query_params(
            "CREATE probe SET blob = encoding::base64::decode($b) RETURN AFTER",
            &params,
        )
        .await
        .expect("insert");
    eprintln!("bytes: {rows:?}");
    let blob = &rows[0]["blob"];
    eprintln!("bytes value: {blob:?} ({})", blob);
    let back: Vec<u8> = serde_json::from_value(blob.clone()).expect("Vec<u8> decode");
    assert_eq!(back, vec![1, 2, 3]);
}
#[tokio::test]
pub(crate) async fn supplied_record_id_round_trips() {
    let db = memory().await;
    db.exec("DEFINE TABLE probe SCHEMAFULL; DEFINE FIELD name ON probe TYPE string;")
        .await
        .expect("define table");
    let id = uuid::Uuid::new_v4();
    let params = BTreeMap::from([
        ("id".to_string(), serde_json::json!(id.to_string())),
        ("data".to_string(), serde_json::json!({ "name": "a" })),
    ]);
    let rows: Vec<serde_json::Value> = db
        .query_params(
            "CREATE type::record('probe', <uuid>$id) CONTENT $data RETURN AFTER",
            &params,
        )
        .await
        .expect("insert with supplied id");
    eprintln!("id after insert: {rows:?}");
    assert_eq!(rows[0]["id"], serde_json::json!(id.to_string()));

    let bound: Vec<serde_json::Value> = db
        .query_params(
            "SELECT * FROM type::record('probe', <uuid>$id)",
            &BTreeMap::from([("id".to_string(), serde_json::json!(id.to_string()))]),
        )
        .await
        .expect("select by bound id");
    eprintln!("rows by bound id: {bound:?}");
    assert_eq!(bound.len(), 1);

    let inline: Vec<serde_json::Value> = db
        .query(&format!("SELECT * FROM type::record('probe', u'{id}')"))
        .await
        .expect("select by inline id");
    eprintln!("rows by inline id: {inline:?}");
    assert_eq!(inline.len(), 1);
}
#[tokio::test]
pub(crate) async fn duplicate_is_classified() {
    let db = memory().await;
    db.exec(
        "DEFINE TABLE probe SCHEMAFULL;\
         DEFINE FIELD email ON probe TYPE string;\
         DEFINE INDEX probe_email ON probe FIELDS email UNIQUE;",
    )
    .await
    .expect("define unique table");
    db.exec("CREATE probe SET email = 'a@b.c';")
        .await
        .expect("first insert");
    let err = db
        .exec("CREATE probe SET email = 'a@b.c';")
        .await
        .expect_err("second insert must fail");
    eprintln!("unique error: {err}");
    assert!(matches!(err, DbError::Unique(ref index) if index == "probe_email"));
}
#[tokio::test]
pub(crate) async fn schemaless_rows_round_trip_into_records() {
    let db = memory().await;
    db.apply([
        ProbeRow::define_table(),
        "DEFINE INDEX IF NOT EXISTS probe_row_owner ON probe_row FIELDS owner UNIQUE;",
    ])
    .await
    .expect("schema");

    let at = chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 10, 29, 12, 0, 0).unwrap();
    let owner = uuid::Uuid::new_v4();
    let id = uuid::Uuid::new_v4();
    let row = ProbeRow {
        id,
        owner,
        at,
    };
    let stored = db
        .create("probe_row", Key::Uuid(id), &row)
        .await
        .expect("create");
    assert_eq!(stored.id, id);
    assert_eq!(stored.owner, owner);
    assert_eq!(stored.at, at);

    let found: Vec<ProbeRow> = db
        .query_params(
            "SELECT * FROM probe_row WHERE owner = $owner",
            &BTreeMap::from([("owner".to_string(), serde_json::json!(owner.to_string()))]),
        )
        .await
        .expect("filter by uuid string");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, id);
}
