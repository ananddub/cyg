use super::*;

#[tokio::test]
pub async fn every_migrated_path_is_still_registered() {
    let app = engine_api::auto_router(state()).await;

    let routes: &[(&str, &str)] = &[
        ("GET", "/health"),
        ("GET", "/auto/health"),
        ("GET", "/status"),
        ("GET", "/readyz"),
        ("GET", "/api/rooms"),
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
        ("POST", "/api/matchmake/joinOrCreate/tank-3d"),
        ("POST", "/matchmake/joinOrCreate/tank-3d"),
        ("POST", "/v2/account/authenticate"),
        ("POST", "/v2/account/authenticate/device"),
        ("POST", "/v2/account/authenticate/email"),
        ("POST", "/v2/account/authenticate/custom"),
        ("GET", "/v2/account"),
        ("PUT", "/v2/account"),
        ("DELETE", "/v2/account"),
        ("POST", "/v2/account/session/refresh"),
        ("POST", "/v2/session/logout"),
        ("POST", "/v2/account/link"),
        ("POST", "/v2/account/link/device"),
        ("POST", "/v2/account/link/steam"),
        ("POST", "/v2/account/unlink"),
        ("POST", "/v2/account/unlink/device"),
        ("POST", "/v2/account/unlink/steam"),
        ("PUT", "/v2/storage"),
        ("POST", "/v2/storage"),
        ("PUT", "/v2/storage/delete"),
        ("GET", "/v2/storage/collection"),
        (
            "GET",
            "/v2/storage/collection/00000000-0000-0000-0000-000000000000",
        ),
        (
            "GET",
            "/v2/storage/collection/key/00000000-0000-0000-0000-000000000000",
        ),
        ("GET", "/v2/inventory"),
        ("POST", "/v2/inventory/equip"),
        ("POST", "/v2/inventory/consume"),
        ("POST", "/v2/inventory/grant"),
        ("POST", "/v2/inventory/define"),
        ("POST", "/v2/leaderboard/board"),
        ("GET", "/v2/leaderboard/board"),
        ("DELETE", "/v2/leaderboard/board"),
        (
            "GET",
            "/v2/leaderboard/board/owner/00000000-0000-0000-0000-000000000000",
        ),
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
        ("GET", "/v2/friend"),
        ("POST", "/v2/friend"),
        ("DELETE", "/v2/friend"),
        ("POST", "/v2/friend/block"),
        ("GET", "/v2/friend/friends"),
        ("POST", "/v2/friend/facebook"),
        ("POST", "/v2/friend/steam"),
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
        ("GET", "/v2/notification"),
        ("DELETE", "/v2/notification"),
        ("DELETE", "/v2/notification/n"),
        ("GET", "/v2/channel/2.arena"),
        ("POST", "/v2/channel/2.arena/message"),
        ("PUT", "/v2/channel/2.arena/message/m"),
        ("DELETE", "/v2/channel/2.arena/message/m"),
        ("GET", "/v2/user"),
        ("POST", "/v2/user"),
        ("GET", "/v2/user/00000000-0000-0000-0000-000000000000"),
        ("POST", "/v2/console/authenticate"),
        ("POST", "/v2/console/authenticate/logout"),
        ("GET", "/v2/console/account"),
        (
            "GET",
            "/v2/console/account/00000000-0000-0000-0000-000000000000",
        ),
        (
            "GET",
            "/v2/console/account/00000000-0000-0000-0000-000000000000/note",
        ),
        (
            "GET",
            "/v2/console/account/00000000-0000-0000-0000-000000000000/wallet",
        ),
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
        ("POST", "/v2/console/account/import"),
        (
            "DELETE",
            "/v2/console/account/00000000-0000-0000-0000-000000000000",
        ),
        (
            "GET",
            "/v2/console/account/00000000-0000-0000-0000-000000000000/export",
        ),
        (
            "POST",
            "/v2/console/account/00000000-0000-0000-0000-000000000000/ban",
        ),
        (
            "POST",
            "/v2/console/account/00000000-0000-0000-0000-000000000000/unban",
        ),
        (
            "POST",
            "/v2/console/account/00000000-0000-0000-0000-000000000000/note",
        ),
        (
            "DELETE",
            "/v2/console/account/00000000-0000-0000-0000-000000000000/note/00000000-0000-0000-0000-000000000000",
        ),
        (
            "POST",
            "/v2/console/account/00000000-0000-0000-0000-000000000000/wallet",
        ),
        ("POST", "/v2/console/user"),
        (
            "PUT",
            "/v2/console/user/00000000-0000-0000-0000-000000000000",
        ),
        (
            "DELETE",
            "/v2/console/user/00000000-0000-0000-0000-000000000000",
        ),
        (
            "POST",
            "/v2/console/user/00000000-0000-0000-0000-000000000000/password",
        ),
        (
            "POST",
            "/v2/console/user/00000000-0000-0000-0000-000000000000/disable",
        ),
        (
            "POST",
            "/v2/console/user/00000000-0000-0000-0000-000000000000/enable",
        ),
        (
            "DELETE",
            "/v2/console/user/00000000-0000-0000-0000-000000000000/mfa",
        ),
        (
            "DELETE",
            "/v2/console/group/00000000-0000-0000-0000-000000000000",
        ),
        (
            "DELETE",
            "/v2/console/storage/object/c/k/00000000-0000-0000-0000-000000000000",
        ),
        ("DELETE", "/v2/console/storage/collection/c"),
        ("POST", "/v2/console/notification"),
        (
            "DELETE",
            "/v2/console/notification/00000000-0000-0000-0000-000000000000",
        ),
        ("POST", "/v2/console/iap/apple/validate"),
        ("DELETE", "/v2/console/iap/apple/reset"),
        ("DELETE", "/v2/console/subscription/season-pass"),
        ("PUT", "/v2/console/setting"),
        ("DELETE", "/v2/console/setting/example"),
        ("POST", "/v2/console/acl/template"),
        (
            "PUT",
            "/v2/console/acl/template/00000000-0000-0000-0000-000000000000",
        ),
        (
            "DELETE",
            "/v2/console/acl/template/00000000-0000-0000-0000-000000000000",
        ),
    ];

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
