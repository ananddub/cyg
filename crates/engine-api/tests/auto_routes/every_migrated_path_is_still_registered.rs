use super::*;

/// The route table snapshot every surface test agrees on: the router must
/// register all of it, and the served OpenAPI spec must describe all of it.
pub fn pinned_routes() -> &'static [(&'static str, &'static str)] {
    &[
        ("GET", "/health"),
        ("GET", "/auto/health"),
        ("GET", "/status"),
        ("GET", "/readyz"),
        ("GET", "/lobby/rooms"),
        ("GET", "/api/dashboard/overview"),
        ("GET", "/api/dashboard/rooms"),
        ("GET", "/api/dashboard/workers"),
        ("GET", "/api/dashboard/clients"),
        ("GET", "/api/dashboard/ticks"),
        ("GET", "/dashboard"),
        ("GET", "/dashboard/favicon.svg"),
        ("GET", "/dashboard/icons.svg"),
        ("GET", "/dashboard/logo.png"),
        ("GET", "/dashboard/assets/app.js"),
        ("GET", "/ws"),
        ("GET", "/ws/signal"),
        ("GET", "/ws/dashboard"),
        ("POST", "/matchmake/joinOrCreate/tank-3d"),
        ("POST", "/v1/account/authenticate"),
        ("POST", "/v1/account/authenticate/device"),
        ("POST", "/v1/account/authenticate/email"),
        ("POST", "/v1/account/authenticate/custom"),
        ("GET", "/v1/account"),
        ("PUT", "/v1/account"),
        ("DELETE", "/v1/account"),
        ("GET", "/v1/account/export"),
        ("POST", "/v1/account/delete"),
        ("POST", "/v1/account/session/refresh"),
        ("POST", "/v1/session/logout"),
        ("POST", "/v1/account/link"),
        ("POST", "/v1/account/link/device"),
        ("POST", "/v1/account/link/steam"),
        ("POST", "/v1/account/unlink"),
        ("POST", "/v1/account/unlink/device"),
        ("POST", "/v1/account/unlink/steam"),
        ("PUT", "/v1/storage"),
        ("POST", "/v1/storage"),
        ("PUT", "/v1/storage/delete"),
        ("GET", "/v1/storage/collection"),
        (
            "GET",
            "/v1/storage/collection/00000000-0000-0000-0000-000000000000",
        ),
        (
            "GET",
            "/v1/storage/collection/key/00000000-0000-0000-0000-000000000000",
        ),
        ("GET", "/v1/inventory"),
        ("POST", "/v1/inventory/equip"),
        ("POST", "/v1/inventory/consume"),
        ("POST", "/v1/inventory/grant"),
        ("POST", "/v1/inventory/define"),
        ("POST", "/v1/leaderboard/board"),
        ("GET", "/v1/leaderboard/board"),
        ("DELETE", "/v1/leaderboard/board"),
        (
            "GET",
            "/v1/leaderboard/board/owner/00000000-0000-0000-0000-000000000000",
        ),
        ("GET", "/v1/tournament"),
        ("GET", "/v1/tournament/tourney"),
        ("POST", "/v1/tournament/tourney"),
        ("PUT", "/v1/tournament/tourney"),
        ("DELETE", "/v1/tournament/tourney"),
        (
            "GET",
            "/v1/tournament/tourney/owner/00000000-0000-0000-0000-000000000000",
        ),
        ("POST", "/v1/tournament/tourney/join"),
        ("POST", "/v1/iap"),
        ("POST", "/v1/iap/purchase/apple"),
        ("POST", "/v1/iap/purchase/google"),
        ("POST", "/v1/iap/purchase/huawei"),
        ("POST", "/v1/iap/purchase/facebookinstant"),
        ("POST", "/v1/iap/purchase/samsung"),
        ("GET", "/v1/iap/purchase"),
        ("POST", "/v1/iap/subscription/apple"),
        ("POST", "/v1/iap/subscription/google"),
        ("POST", "/v1/iap/subscription/huawei"),
        ("GET", "/v1/iap/subscription"),
        ("GET", "/v1/iap/subscription/season-pass"),
        ("GET", "/v1/iap/subscription/active"),
        ("GET", "/v1/friend"),
        ("POST", "/v1/friend"),
        ("DELETE", "/v1/friend"),
        ("POST", "/v1/friend/block"),
        ("GET", "/v1/friend/friends"),
        ("POST", "/v1/friend/facebook"),
        ("POST", "/v1/friend/steam"),
        ("GET", "/v1/group"),
        ("POST", "/v1/group"),
        ("GET", "/v1/group/g"),
        ("PUT", "/v1/group/g"),
        ("DELETE", "/v1/group/g"),
        ("POST", "/v1/group/g/join"),
        ("POST", "/v1/group/g/leave"),
        ("GET", "/v1/group/g/user"),
        ("POST", "/v1/group/g/add"),
        ("POST", "/v1/group/g/kick"),
        ("POST", "/v1/group/g/ban"),
        ("POST", "/v1/group/g/promote"),
        ("POST", "/v1/group/g/demote"),
        ("GET", "/v1/user/u/group"),
        ("GET", "/v1/notification"),
        ("DELETE", "/v1/notification"),
        ("DELETE", "/v1/notification/n"),
        ("POST", "/v1/reports"),
        ("GET", "/v1/reports"),
        (
            "PUT",
            "/v1/reports/00000000-0000-0000-0000-000000000000/status",
        ),
        ("GET", "/v1/channel/2.arena"),
        ("POST", "/v1/channel/2.arena/message"),
        ("PUT", "/v1/channel/2.arena/message/m"),
        ("DELETE", "/v1/channel/2.arena/message/m"),
        ("GET", "/v1/user"),
        ("POST", "/v1/user"),
        ("GET", "/v1/user/00000000-0000-0000-0000-000000000000"),
        ("POST", "/v1/console/authenticate"),
        ("POST", "/v1/console/authenticate/logout"),
        ("GET", "/v1/console/account"),
        (
            "GET",
            "/v1/console/account/00000000-0000-0000-0000-000000000000",
        ),
        (
            "GET",
            "/v1/console/account/00000000-0000-0000-0000-000000000000/note",
        ),
        (
            "GET",
            "/v1/console/account/00000000-0000-0000-0000-000000000000/wallet",
        ),
        ("GET", "/v1/console/user"),
        ("GET", "/v1/console/group"),
        ("GET", "/v1/console/group/g"),
        ("GET", "/v1/console/group/g/member"),
        ("GET", "/v1/console/leaderboard"),
        ("GET", "/v1/console/leaderboard/b"),
        ("DELETE", "/v1/console/leaderboard/b"),
        ("GET", "/v1/console/leaderboard/b/records"),
        ("GET", "/v1/console/tournament"),
        ("POST", "/v1/console/tournament"),
        ("PUT", "/v1/console/tournament/t"),
        ("DELETE", "/v1/console/tournament/t"),
        ("GET", "/v1/console/storage"),
        ("GET", "/v1/console/storage/collections"),
        ("GET", "/v1/console/storage/c"),
        ("GET", "/v1/console/notification"),
        ("GET", "/v1/console/purchase"),
        ("GET", "/v1/console/subscription"),
        ("GET", "/v1/console/setting"),
        ("GET", "/v1/console/room"),
        ("GET", "/v1/console/audit/log"),
        ("GET", "/v1/console/acl/template"),
        ("GET", "/v1/console/status"),
        ("GET", "/v1/console/route"),
        ("POST", "/v1/console/mfa"),
        ("POST", "/v1/console/mfa/verify"),
        ("DELETE", "/v1/console/mfa"),
        ("POST", "/v1/console/account/import"),
        (
            "DELETE",
            "/v1/console/account/00000000-0000-0000-0000-000000000000",
        ),
        (
            "GET",
            "/v1/console/account/00000000-0000-0000-0000-000000000000/export",
        ),
        (
            "POST",
            "/v1/console/account/00000000-0000-0000-0000-000000000000/ban",
        ),
        (
            "POST",
            "/v1/console/account/00000000-0000-0000-0000-000000000000/unban",
        ),
        (
            "POST",
            "/v1/console/account/00000000-0000-0000-0000-000000000000/note",
        ),
        (
            "DELETE",
            "/v1/console/account/00000000-0000-0000-0000-000000000000/note/00000000-0000-0000-0000-000000000000",
        ),
        (
            "POST",
            "/v1/console/account/00000000-0000-0000-0000-000000000000/wallet",
        ),
        ("POST", "/v1/console/user"),
        (
            "PUT",
            "/v1/console/user/00000000-0000-0000-0000-000000000000",
        ),
        (
            "DELETE",
            "/v1/console/user/00000000-0000-0000-0000-000000000000",
        ),
        (
            "POST",
            "/v1/console/user/00000000-0000-0000-0000-000000000000/password",
        ),
        (
            "POST",
            "/v1/console/user/00000000-0000-0000-0000-000000000000/disable",
        ),
        (
            "POST",
            "/v1/console/user/00000000-0000-0000-0000-000000000000/enable",
        ),
        (
            "DELETE",
            "/v1/console/user/00000000-0000-0000-0000-000000000000/mfa",
        ),
        (
            "DELETE",
            "/v1/console/group/00000000-0000-0000-0000-000000000000",
        ),
        (
            "DELETE",
            "/v1/console/storage/object/c/k/00000000-0000-0000-0000-000000000000",
        ),
        ("DELETE", "/v1/console/storage/collection/c"),
        ("POST", "/v1/console/notification"),
        (
            "DELETE",
            "/v1/console/notification/00000000-0000-0000-0000-000000000000",
        ),
        ("POST", "/v1/console/iap/apple/validate"),
        ("DELETE", "/v1/console/iap/apple/reset"),
        ("DELETE", "/v1/console/subscription/season-pass"),
        ("PUT", "/v1/console/setting"),
        ("DELETE", "/v1/console/setting/example"),
        ("POST", "/v1/console/acl/template"),
        (
            "PUT",
            "/v1/console/acl/template/00000000-0000-0000-0000-000000000000",
        ),
        (
            "DELETE",
            "/v1/console/acl/template/00000000-0000-0000-0000-000000000000",
        ),
        ("POST", "/v1/marketplace/listings"),
        (
            "POST",
            "/v1/marketplace/listings/00000000-0000-0000-0000-000000000000/buy",
        ),
        (
            "POST",
            "/v1/marketplace/listings/00000000-0000-0000-0000-000000000000/cancel",
        ),
        ("POST", "/v1/wager/create"),
        (
            "POST",
            "/v1/wager/00000000-0000-0000-0000-000000000000/settle",
        ),
        (
            "POST",
            "/v1/wager/00000000-0000-0000-0000-000000000000/refund",
        ),
        ("POST", "/v1/events"),
        ("GET", "/v1/quest"),
        ("POST", "/v1/quest/claim"),
        ("POST", "/v1/account/daily/claim"),
        ("GET", "/v1/console/quests"),
        ("POST", "/v1/console/quests"),
        ("PUT", "/v1/console/quests/example"),
        ("DELETE", "/v1/console/quests/example"),
        ("GET", "/v1/console/seasons"),
        ("POST", "/v1/console/seasons"),
        ("PUT", "/v1/console/seasons/example"),
        ("DELETE", "/v1/console/seasons/example"),
        ("GET", "/v1/console/webhooks"),
        ("POST", "/v1/console/webhooks"),
        ("GET", "/v1/console/webhooks/spec"),
        ("PUT", "/v1/console/webhooks/example"),
        ("DELETE", "/v1/console/webhooks/example"),
        ("GET", "/v1/console/webhooks/example/deliveries"),
        (
            "POST",
            "/v1/console/webhooks/example/deliveries/example/retry",
        ),
        ("GET", "/v1/match/arena/events"),
        ("GET", "/v1/match/arena/history"),
        ("GET", "/v1/console/desync"),
        ("GET", "/v1/console/match/arena/replay"),
        ("GET", "/v1/console/rules"),
        ("POST", "/v1/console/rules"),
        ("POST", "/v1/console/rules/dry-run"),
        ("GET", "/v1/console/rules/example"),
        ("POST", "/v1/console/rules/example/publish/1"),
        ("DELETE", "/v1/console/rules/example"),
        (
            "GET",
            "/v1/console/games/00000000-0000-0000-0000-000000000000/config/history",
        ),
        (
            "GET",
            "/v1/console/games/00000000-0000-0000-0000-000000000000/config/1",
        ),
        (
            "POST",
            "/v1/console/games/00000000-0000-0000-0000-000000000000/config/rollback/1",
        ),
        (
            "GET",
            "/v1/console/timeline/00000000-0000-0000-0000-000000000000",
        ),
        ("GET", "/v1/console/economy/summary"),
        (
            "GET",
            "/v1/console/games/00000000-0000-0000-0000-000000000000/maps/1/preview",
        ),
        ("GET", "/openapi.json"),
        // /console/api-docs is a nested UI mount, not a controller route: it
        // answers GET and 404s everything else, so the 405 sweep skips it.
        ("GET", "/v1/console/logs/recent"),
        (
            "POST",
            "/v1/match/00000000-0000-0000-0000-000000000000/result",
        ),
        ("GET", "/v1/badges"),
        ("GET", "/v1/console/badges"),
        ("POST", "/v1/console/badges"),
        ("PUT", "/v1/console/badges/example"),
        ("DELETE", "/v1/console/badges/example"),
        ("GET", "/v1/flags"),
        ("GET", "/v1/console/flags"),
        ("POST", "/v1/console/flags"),
        ("PUT", "/v1/console/flags/example"),
        ("DELETE", "/v1/console/flags/example"),
        ("POST", "/v1/console/flags/example/rollout"),
        ("POST", "/v1/console/flags/example/stop"),
        ("GET", "/v1/console/flags/example/variants"),
        ("POST", "/v1/console/flags/example/variants"),
        ("PUT", "/v1/console/flags/example/variants/control"),
        ("DELETE", "/v1/console/flags/example/variants/control"),
        ("GET", "/v1/console/flags/example/overrides"),
        ("PUT", "/v1/console/flags/example/overrides"),
        (
            "DELETE",
            "/v1/console/flags/example/overrides/00000000-0000-0000-0000-000000000000",
        ),
        ("GET", "/v1/notify/preferences"),
        ("PUT", "/v1/notify/preferences"),
        ("GET", "/v1/notify/unsubscribe"),
        ("POST", "/v1/notify/test"),
        (
            "GET",
            "/v1/notify/test/00000000-0000-0000-0000-000000000000",
        ),
    ]
}

#[tokio::test]
pub async fn every_migrated_path_is_still_registered() {
    let app = engine_api::auto_router(state()).await;
    let routes = pinned_routes();

    let mut paths: Vec<&str> = routes.iter().map(|(_, path)| *path).collect();
    paths.sort_unstable();
    paths.dedup();

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
