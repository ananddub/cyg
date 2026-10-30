pub(crate) mod db {
    use super::super::*;

    pub(crate) async fn state() -> Option<(ApiState, std::sync::Arc<Datastore>)> {
        let db = std::sync::Arc::new(
            Datastore::open("mem://", "cyg", "cyg", None, None, KvBudget::default())
                .await
                .expect("the embedded store opens"),
        );
        crate::services::games::schema::ensure_schema(&db)
            .await
            .expect("the tenancy tables must be creatable on the embedded store");

        let mut state = v2_state();
        state.q = Some(std::sync::Arc::new(crate::repositories::db::Queries::new(
            db.clone(),
        )));
        state.db = Some(db.clone());
        state.blobs = Some(std::sync::Arc::new(
            crate::db::BlobStore::open("memory://", "mem://", Some(db.clone()))
                .await
                .expect("the memory bucket is definable"),
        ));
        state.realtime =
            std::sync::Arc::new(crate::infra::realtime::RealtimeHub::new(Some(db.clone())));
        Some((state, db))
    }

    pub(crate) fn fixture_name(label: &str) -> String {
        format!("{TEST_PREFIX} {label} {}", Uuid::new_v4())
    }

    pub(crate) async fn create(
        state: &ApiState,
        label: &str,
    ) -> crate::dto::console::games::GameDto {
        create_engine(state, label, None).await
    }

    pub(crate) async fn create_engine(
        state: &ApiState,
        label: &str,
        engine: Option<&str>,
    ) -> crate::dto::console::games::GameDto {
        let body = CreateGameRequest {
            name: fixture_name(label),
            engine_type: engine.map(str::to_owned),
            ..Default::default()
        };
        GamesService::create_game(state, &body)
            .await
            .expect("a named game registers")
    }

    pub(crate) async fn create_wasm(
        state: &ApiState,
        label: &str,
    ) -> crate::dto::console::games::GameDto {
        create_engine(state, label, Some("custom")).await
    }

    pub(crate) async fn create_from_value(
        state: &ApiState,
        body: Value,
    ) -> crate::dto::console::games::GameDto {
        let request: CreateGameRequest =
            serde_json::from_value(body).expect("the fixture body is a create request");
        GamesService::create_game(state, &request)
            .await
            .expect("the fixture body registers a game")
    }

    pub(crate) async fn forget(db: &Datastore, game_id: Uuid) {
        db.exec_params(
            "DELETE type::record('games', <uuid>$id) RETURN NOTHING",
            &crate::repositories::db::params! { id = game_id.to_string() },
        )
        .await
        .expect("the fixture game is deletable");
    }

    pub(crate) async fn insert_module(
        state: &ApiState,
        db: &Datastore,
        game_id: Uuid,
        version: i32,
        filename: &str,
        binary: &[u8],
        source: Option<&str>,
        sha: &str,
        active: bool,
    ) -> Result<(), crate::db::DbError> {
        let key = format!("wasm/{game_id}/{version}.wasm");
        state
            .blobs
            .as_ref()
            .expect("the test plane has a blob store")
            .put(&key, binary)
            .await
            .expect("the module payload stores");
        let mut row = json!({
            "game_id": game_id.to_string(),
            "version": version,
            "filename": filename,
            "blob_key": key,
            "size_bytes": binary.len() as i64,
            "sha256": sha,
            "is_active": active,
            "created_at": time::encode(Utc::now()),
        });
        if let Some(source) = source {
            row["source_code"] = json!(source);
        }
        let id = Uuid::new_v4().to_string();
        db.exec_params(
            "CREATE type::record('room_modules', <uuid>$id) CONTENT $row RETURN NOTHING",
            &crate::repositories::db::params! { id = id, row = row },
        )
        .await
    }
}
pub(crate) fn skipped_without_database() {
    tracing::info!("skipping: the embedded `mem://` store did not open");
}
#[tokio::test]
pub(crate) async fn ensure_schema_creates_the_tables_and_is_idempotent() {
    let Some((_state, pool)) = db::state().await else {
        return skipped_without_database();
    };

    crate::services::games::schema::ensure_schema(&pool)
        .await
        .expect("a second ensure_schema changes nothing and reports no error");
    crate::services::games::schema::ensure_schema(&pool)
        .await
        .expect("and a third, for good measure");

    let info = pool
        .raw_all("INFO FOR DB")
        .await
        .expect("the table listing is readable");
    let rows = info[0].as_array().expect("one result row");
    let tables = rows[0]["tables"].as_object().expect("a table map");
    for table in ["games", "room_modules"] {
        assert!(tables.contains_key(table), "`{table}` must exist");
    }
}
