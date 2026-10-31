use super::*;

#[test]
pub(crate) fn an_unknown_unity_settings_format_is_refused() {
    let server = V2Server::start(v2_state());
    let reply = server.get(
        &format!(
            "/v2/console/games/{}/unity-settings?format=xml",
            Uuid::new_v4()
        ),
        Some(&admin_token()),
    );
    let message = reply.refusal(INVALID_ARGUMENT);
    assert!(
        message.contains("Unknown unity-settings format 'xml'"),
        "{message}"
    );
}
#[tokio::test]
pub(crate) async fn the_unity_settings_of_a_missing_game_are_not_found() {
    let Some((state, _pool)) = db::state().await else {
        return skipped_without_database();
    };
    let err =
        GamesService::unity_settings(&state, Uuid::new_v4(), false, &axum::http::HeaderMap::new())
            .await
            .expect_err("no game, no settings");
    assert_eq!(err.kind, NOT_FOUND, "{err:?}");
}
