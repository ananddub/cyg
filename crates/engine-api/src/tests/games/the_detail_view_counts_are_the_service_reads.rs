use super::*;

#[tokio::test]
pub(crate) async fn the_detail_view_counts_are_the_service_reads() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };

    let game = db::create(&state, "agreement").await;
    db::insert_module(
        &state,
        &pool,
        game.id,
        3,
        "room.wasm",
        &wasm_bytes(),
        None,
        &hex::encode([9u8; 32]),
        true,
    )
    .await
    .expect("an active module at version 3");

    let detail = GamesService::get_game(&state, game.id)
        .await
        .expect("the detail view reads");
    assert_eq!(
        detail.module_count,
        GamesService::module_count(&state, game.id)
            .await
            .expect("count")
    );
    assert_eq!(
        detail.active_module_version,
        Some(3),
        "the version the detail view prints is the version a room would boot"
    );
    assert_eq!(detail.module_count, 1);

    db::forget(&pool, game.id).await;
}
pub(crate) async fn fetch_raw(
    pool: &std::sync::Arc<Datastore>,
    method: &str,
    path: &str,
    token: &str,
) -> (u16, Vec<(String, String)>, String) {
    use tower::ServiceExt;

    let mut state = v2_state();
    state.q = Some(std::sync::Arc::new(crate::repositories::db::Queries::new(
        pool.clone(),
    )));
    state.db = Some(pool.clone());

    let app = crate::auto_router(state).await;
    let request = axum::http::Request::builder()
        .method(method)
        .uri(path)
        .header(axum::http::header::HOST, "console.test")
        .header(axum::http::header::AUTHORIZATION, token)
        .body(axum::body::Body::empty())
        .expect("a hand-built request");
    let reply = app.oneshot(request).await.expect("the router answers");
    let status = reply.status().as_u16();
    let headers = reply
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_lowercase(),
                value.to_str().unwrap_or_default().to_owned(),
            )
        })
        .collect();
    let bytes = http_body_util::BodyExt::collect(reply.into_body())
        .await
        .expect("a body")
        .to_bytes();
    (
        status,
        headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}
pub(crate) fn yaml_field(document: &str, key: &str) -> String {
    document
        .lines()
        .find_map(|line| {
            let (name, value) = line.trim_start().split_once(':')?;
            (name.trim() == key).then(|| value.trim().to_owned())
        })
        .unwrap_or_else(|| panic!("`{key}` is not in the asset:\n{document}"))
}
