use super::server::TestServer;
use super::test_state;
use crate::ApiState;
use crate::matchmake::router;
use axum::Router;
use axum::routing::post;

/// The plane composes these routes with the other groups
/// (`ControlPlane::build_router` merges them), so the ceiling must travel with
/// *this* Router's layer rather than only guarding the standalone one.
#[test]
fn the_body_limit_and_the_seat_survive_being_merged_into_the_plane() {
    let state: ApiState = test_state(1000, 1000);
    let neighbour = Router::new().route("/ping", post(|| async { "pong" }));
    let routes = router(&state).merge(neighbour);
    let server = TestServer::serve(state, routes);

    let (status, _, body) = server.post("/ping", "");
    assert_eq!((status, body.as_str()), (200, "pong"), "the neighbour still routes");

    let (status, _, body) = server.post("/api/matchmake/joinOrCreate/tank-3d", "{}");
    assert_eq!(status, 200, "merged seat: {body}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["roomId"].as_str().unwrap(), "tank-3d");

    let big = "x".repeat(super::MAX_BODY_BYTES + 1024);
    let (status, _, _) = server.post(
        "/api/matchmake/reconnect/tank-3d",
        &format!("{{\"reconnectionToken\":\"{big}\"}}"),
    );
    assert_eq!(status, 413, "the layer is not flattened away by the merge");
}
