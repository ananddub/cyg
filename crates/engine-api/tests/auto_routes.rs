//! The migrated surface: every route group is registered by `#[controller]`
//! attributes at compile time, and `engine_api::auto_router` resolves them the
//! same way `serve` does.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

// A whole plane built by hand (mirrors the unit-test fixture): every field of
// `ApiState` is public precisely so routing can be tested without binding.
fn state() -> engine_api::ApiState {
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    use engine_bus::Bus;
    use engine_config::{ControlConfig, GameConfig};

    let game = GameConfig::default();
    let control = ControlConfig::default();
    let bus = Bus::new(16, 16);
    engine_api::ApiState {
        statics: Arc::new(engine_api::services::static_routes::StaticServer::new(
            Path::new("."),
            &game,
        )),
        game: Arc::new(game),
        control: Arc::new(control),
        signals: engine_api::infra::signaling::SignalHub::new("[]".into()),
        seats: Arc::new(Mutex::new(engine_api::infra::seats::SeatRegistry::new("test-secret"))),
        limiter: Arc::new(Mutex::new(engine_api::infra::rate_limit::IpRateLimiter::new(20, 40))),
        port: 8124,
        status_rx: Arc::new(engine_api::OnceReceiver::new(bus.status_rx.clone())),
        latest_status: Arc::new(Mutex::new(None)),
        bus,
        started: Instant::now(),
        db: None,
        q: None,
        sessions: Arc::new(engine_api::infra::session_cache::SessionCache::new(60, 3600)),
    }
}

#[tokio::test]
async fn auto_registered_health_route_dispatches() {
    let app = engine_api::auto_router(state()).await;

    let response = app
        .clone()
        .oneshot(Request::get("/auto/health").body(Body::empty()).unwrap())
        .await
        .expect("dispatch");
    assert_eq!(response.status(), StatusCode::OK);

    // The migrated surface: same paths as before, now registered by
    // `#[controller]` instead of a hand-written router per group.
    for path in ["/status", "/api/rooms", "/health", "/v2/user", "/v2/inventory"] {
        let response = app
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .expect("dispatch");
        // `/v2/user` and `/v2/inventory` need a session token, but they must not 404:
        // a 401 proves the route exists and reached its extractor.
        let ok = if path == "/v2/user" || path == "/v2/inventory" {
            response.status() == StatusCode::UNAUTHORIZED
        } else {
            response.status() == StatusCode::OK
        };
        assert!(ok, "{path} served as {}", response.status());
    }
}

/// Two planes alive at the same time must not see each other's state: the
/// per-request attachment is what keeps concurrent tests (and two planes in
/// one process) honest.
#[tokio::test]
async fn two_planes_serve_their_own_state() {
    let mut first = state();
    first.port = 1111;
    let mut second = state();
    second.port = 2222;

    let a = engine_api::auto_router(first).await;
    let b = engine_api::auto_router(second).await;

    for (app, port) in [(a, 1111u64), (b, 2222u64)] {
        let body = app
            .oneshot(Request::get("/api/rooms").body(Body::empty()).unwrap())
            .await
            .expect("dispatch")
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let room: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(room[0]["controlPort"].as_u64(), Some(port));
    }
}

/// Every path the migration moved must still be *registered*, with its method.
///
/// The probe uses a method the surface never claims (`PATCH`) and reads axum's
/// own answer: a registered path with an unregistered method is `405`; a path
/// that no longer exists is `404`. That distinction is routing, whereas a
/// handler's own `404` ("no such user", "unknown RPC function") is the group's
/// business and must not be mistaken for a missing route.
///
/// The list is the pre-migration route table, so a converter who drops or
/// renames a path fails here instead of failing a client.
#[tokio::test]
async fn every_migrated_path_is_still_registered() {
    let app = engine_api::auto_router(state()).await;

    let routes: &[(&str, &str)] = &[
        // telemetry + auto smoke
        ("GET", "/health"),
        ("GET", "/auto/health"),
        ("GET", "/status"),
        ("GET", "/api/rooms"),
        // dashboard REST
        ("GET", "/api/dashboard/overview"),
        ("GET", "/api/dashboard/rooms"),
        ("GET", "/api/dashboard/workers"),
        ("GET", "/api/dashboard/clients"),
        ("GET", "/api/dashboard/ticks"),
        // dashboard assets
        ("GET", "/dashboard"),
        ("GET", "/dashboard/favicon.svg"),
        ("GET", "/dashboard/icons.svg"),
        ("GET", "/dashboard/logo.png"),
        ("GET", "/dashboard/assets/app.js"),
        // websockets
        ("GET", "/ws"),
        ("GET", "/ws/signal"),
        ("GET", "/ws/dashboard"),
        // matchmake (both spellings)
        ("POST", "/api/matchmake/joinOrCreate/tank-3d"),
        ("POST", "/matchmake/joinOrCreate/tank-3d"),
        // v2 auth / account / session
        ("POST", "/v2/account/authenticate"),
        ("POST", "/v2/account/authenticate/device"),
        ("POST", "/v2/account/authenticate/email"),
        ("POST", "/v2/account/authenticate/custom"),
        ("GET", "/v2/account"),
        ("PUT", "/v2/account"),
        ("DELETE", "/v2/account"),
        ("POST", "/v2/account/session/refresh"),
        ("POST", "/v2/session/logout"),
        // v2 link / unlink
        ("POST", "/v2/account/link"),
        ("POST", "/v2/account/link/device"),
        ("POST", "/v2/account/link/steam"),
        ("POST", "/v2/account/unlink"),
        ("POST", "/v2/account/unlink/device"),
        ("POST", "/v2/account/unlink/steam"),
        // v2 storage
        ("PUT", "/v2/storage"),
        ("POST", "/v2/storage"),
        ("PUT", "/v2/storage/delete"),
        ("GET", "/v2/storage/collection"),
        ("GET", "/v2/storage/collection/00000000-0000-0000-0000-000000000000"),
        ("GET", "/v2/storage/collection/key/00000000-0000-0000-0000-000000000000"),
        // v2 inventory
        ("GET", "/v2/inventory"),
        ("POST", "/v2/inventory/equip"),
        ("POST", "/v2/inventory/consume"),
        ("POST", "/v2/inventory/grant"),
        ("POST", "/v2/inventory/define"),
        // v2 leaderboard
        ("POST", "/v2/leaderboard/board"),
        ("GET", "/v2/leaderboard/board"),
        ("DELETE", "/v2/leaderboard/board"),
        (
            "GET",
            "/v2/leaderboard/board/owner/00000000-0000-0000-0000-000000000000",
        ),
        // v2 tournament
        ("GET", "/v2/tournament"),
        ("GET", "/v2/tournament/tourney"),
        ("POST", "/v2/tournament/tourney"),
        ("PUT", "/v2/tournament/tourney"),
        ("DELETE", "/v2/tournament/tourney"),
        (
            "GET",
            "/v2/tournament/tourney/owner/00000000-0000-0000-0000-000000000000",
        ),
        ("POST", "/v2/tournament/tourney/join"),
        // v2 iap
        ("POST", "/v2/iap"),
        ("POST", "/v2/iap/purchase/apple"),
        ("POST", "/v2/iap/purchase/google"),
        ("POST", "/v2/iap/purchase/huawei"),
        ("POST", "/v2/iap/purchase/facebookinstant"),
        ("POST", "/v2/iap/purchase/samsung"),
        ("GET", "/v2/iap/purchase"),
        ("POST", "/v2/iap/subscription/apple"),
        ("POST", "/v2/iap/subscription/google"),
        ("POST", "/v2/iap/subscription/huawei"),
        ("GET", "/v2/iap/subscription"),
        ("GET", "/v2/iap/subscription/season-pass"),
        // v2 friend
        ("GET", "/v2/friend"),
        ("POST", "/v2/friend"),
        ("DELETE", "/v2/friend"),
        ("POST", "/v2/friend/block"),
        ("GET", "/v2/friend/friends"),
        ("POST", "/v2/friend/facebook"),
        ("POST", "/v2/friend/steam"),
        // v2 group
        ("GET", "/v2/group"),
        ("POST", "/v2/group"),
        ("GET", "/v2/group/g"),
        ("PUT", "/v2/group/g"),
        ("DELETE", "/v2/group/g"),
        ("POST", "/v2/group/g/join"),
        ("POST", "/v2/group/g/leave"),
        ("GET", "/v2/group/g/user"),
        ("POST", "/v2/group/g/add"),
        ("POST", "/v2/group/g/kick"),
        ("POST", "/v2/group/g/ban"),
        ("POST", "/v2/group/g/promote"),
        ("POST", "/v2/group/g/demote"),
        ("GET", "/v2/user/u/group"),
        // v2 notification
        ("GET", "/v2/notification"),
        ("DELETE", "/v2/notification"),
        ("DELETE", "/v2/notification/n"),
        // v2 channel
        ("GET", "/v2/channel/2.arena"),
        ("POST", "/v2/channel/2.arena/message"),
        ("PUT", "/v2/channel/2.arena/message/m"),
        ("DELETE", "/v2/channel/2.arena/message/m"),
        // v2 user
        ("GET", "/v2/user"),
        ("POST", "/v2/user"),
        ("GET", "/v2/user/00000000-0000-0000-0000-000000000000"),
        // v2 console (admin)
        ("POST", "/v2/console/authenticate"),
        ("POST", "/v2/console/authenticate/logout"),
        ("GET", "/v2/console/account"),
        ("GET", "/v2/console/account/00000000-0000-0000-0000-000000000000"),
        ("GET", "/v2/console/account/00000000-0000-0000-0000-000000000000/note"),
        ("GET", "/v2/console/account/00000000-0000-0000-0000-000000000000/wallet"),
        ("GET", "/v2/console/user"),
        ("GET", "/v2/console/group"),
        ("GET", "/v2/console/group/g"),
        ("GET", "/v2/console/group/g/member"),
        ("GET", "/v2/console/leaderboard"),
        ("GET", "/v2/console/leaderboard/b"),
        ("DELETE", "/v2/console/leaderboard/b"),
        ("GET", "/v2/console/leaderboard/b/records"),
        ("GET", "/v2/console/tournament"),
        ("POST", "/v2/console/tournament"),
        ("PUT", "/v2/console/tournament/t"),
        ("DELETE", "/v2/console/tournament/t"),
        ("GET", "/v2/console/storage"),
        ("GET", "/v2/console/storage/collections"),
        ("GET", "/v2/console/storage/c"),
        ("GET", "/v2/console/notification"),
        ("GET", "/v2/console/purchase"),
        ("GET", "/v2/console/subscription"),
        ("GET", "/v2/console/setting"),
        ("GET", "/v2/console/audit/log"),
        ("GET", "/v2/console/acl/template"),
        ("GET", "/v2/console/status"),
        ("GET", "/v2/console/route"),
        ("POST", "/v2/console/mfa"),
        ("POST", "/v2/console/mfa/verify"),
        ("DELETE", "/v2/console/mfa"),
        // v2 console (mutations)
        ("POST", "/v2/console/account/import"),
        ("DELETE", "/v2/console/account/00000000-0000-0000-0000-000000000000"),
        ("GET", "/v2/console/account/00000000-0000-0000-0000-000000000000/export"),
        ("POST", "/v2/console/account/00000000-0000-0000-0000-000000000000/ban"),
        ("POST", "/v2/console/account/00000000-0000-0000-0000-000000000000/unban"),
        ("POST", "/v2/console/account/00000000-0000-0000-0000-000000000000/note"),
        (
            "DELETE",
            "/v2/console/account/00000000-0000-0000-0000-000000000000/note/00000000-0000-0000-0000-000000000000",
        ),
        ("POST", "/v2/console/account/00000000-0000-0000-0000-000000000000/wallet"),
        ("POST", "/v2/console/user"),
        ("PUT", "/v2/console/user/00000000-0000-0000-0000-000000000000"),
        ("DELETE", "/v2/console/user/00000000-0000-0000-0000-000000000000"),
        ("POST", "/v2/console/user/00000000-0000-0000-0000-000000000000/password"),
        ("POST", "/v2/console/user/00000000-0000-0000-0000-000000000000/disable"),
        ("POST", "/v2/console/user/00000000-0000-0000-0000-000000000000/enable"),
        ("DELETE", "/v2/console/user/00000000-0000-0000-0000-000000000000/mfa"),
        ("DELETE", "/v2/console/group/00000000-0000-0000-0000-000000000000"),
        (
            "DELETE",
            "/v2/console/storage/object/c/k/00000000-0000-0000-0000-000000000000",
        ),
        ("DELETE", "/v2/console/storage/collection/c"),
        ("POST", "/v2/console/notification"),
        ("DELETE", "/v2/console/notification/00000000-0000-0000-0000-000000000000"),
        ("POST", "/v2/console/iap/apple/validate"),
        ("DELETE", "/v2/console/iap/apple/reset"),
        ("DELETE", "/v2/console/subscription/season-pass"),
        ("PUT", "/v2/console/setting"),
        ("DELETE", "/v2/console/setting/example"),
        ("POST", "/v2/console/acl/template"),
        ("PUT", "/v2/console/acl/template/00000000-0000-0000-0000-000000000000"),
        ("DELETE", "/v2/console/acl/template/00000000-0000-0000-0000-000000000000"),
    ];

    let mut paths: Vec<&str> = routes.iter().map(|(_, path)| *path).collect();
    paths.sort_unstable();
    paths.dedup();

    // A registered path answers `405` to a method it never claims...
    for path in &paths {
        let status = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(*path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("dispatch")
            .status();
        assert_eq!(
            status,
            StatusCode::METHOD_NOT_ALLOWED,
            "PATCH {path}: expected a registered path to answer 405"
        );
    }

    // ...and never `405` to a method it does claim.
    for (method, path) in routes {
        let status = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(*method)
                    .uri(*path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("dispatch")
            .status();
        assert_ne!(
            status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{method} {path} is no longer registered"
        );
    }

    // The fallback still owns everything unmatched.
    let status = app
        .oneshot(
            Request::get("/definitely/not/a/route")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("dispatch")
        .status();
    assert_eq!(status, StatusCode::NOT_FOUND);
}
