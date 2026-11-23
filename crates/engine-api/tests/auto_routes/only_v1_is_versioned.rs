use super::*;

/// The operational surfaces, which are deliberately outside the version: they
/// describe this process, not the game API, and a client should not have to know
/// an API version to read a health check or open a socket.
const OPERATIONAL: &[&str] = &[
    "/",
    "/health",
    "/status",
    "/readyz",
    "/metrics",
    "/openapi.json",
    "/cyg.js",
    "/cyg_core_rs.wasm",
    "/openoxide-client.js",
    "/arena_map.json",
    "/Build/{*rest}",
    "/TemplateData/{*rest}",
    "/unity",
    "/unity/{*rest}",
    // Vendored SDK assets the browser build imports by their literal paths.
    "/packages/sdk/dist/colyseus.js",
    "/node_modules/@colyseus/schema/build/index.mjs",
    "/ws",
    "/ws/signal",
    "/ws/dashboard",
    "/dashboard",
    "/dashboard/{*rest}",
    "/dashboard/favicon.svg",
    "/dashboard/icons.svg",
    "/dashboard/logo.png",
    "/dashboard/assets/{*rest}",
    "/api/dashboard/overview",
    "/api/dashboard/rooms",
    "/api/dashboard/workers",
    "/api/dashboard/clients",
    "/api/dashboard/ticks",
    "/api/dashboard/identity",
    "/auto/health",
    "/lobby/rooms",
    "/queue/join",
    "/queue/status/{ticket}",
    "/queue/leave/{ticket}",
    "/matchmake/{method}/{room}",
];

/// One version, and it is `/v1`.
///
/// Every documented path is either under `/v1` or is an operational surface
/// listed above. This is what keeps the API from growing a second base by
/// accident: a controller authored at `/api/...` or `/v2/...` fails here, and so
/// does one that meant to be versioned but forgot the segment.
#[test]
fn the_spec_publishes_one_version() {
    let spec = engine_api::auto_route::openapi_json();
    let paths = spec["paths"].as_object().expect("spec has a paths map");
    assert!(!paths.is_empty(), "the spec describes some routes");

    let mut stray = Vec::new();
    for path in paths.keys() {
        if path == "/v1" || path.starts_with("/v1/") {
            continue;
        }
        if OPERATIONAL.contains(&path.as_str()) {
            continue;
        }
        stray.push(path.clone());
    }
    stray.sort();
    assert!(
        stray.is_empty(),
        "these paths are neither under /v1 nor operational: {stray:#?}"
    );
}

/// `/v2` is gone. A client that still sends it gets a 404, which is the honest
/// answer: the route does not exist.
#[tokio::test]
async fn the_retired_version_answers_nothing() {
    let spec = engine_api::auto_route::openapi_json();
    let paths = spec["paths"].as_object().expect("spec has a paths map");
    assert!(
        !paths
            .keys()
            .any(|path| path == "/v2" || path.starts_with("/v2/")),
        "the spec must not describe a /v2 surface"
    );

    let app = engine_api::auto_router(state()).await;
    for path in ["/v2/leaderboard", "/v2/account", "/v2/console/account"] {
        let response = app
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .expect("dispatch");
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "{path} must not answer"
        );
    }
}
