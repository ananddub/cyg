use super::*;

#[test]
pub(crate) fn each_new_route_demands_a_console_token() {
    let server = V2Server::start(v2_state());
    let id = Uuid::new_v4();
    for (method, path, body) in [
        (
            "POST",
            format!("/v2/console/user/{id}/mfa/require"),
            String::new(),
        ),
        (
            "DELETE",
            format!("/v2/console/user/{id}/mfa/require"),
            String::new(),
        ),
        (
            "DELETE",
            format!("/v2/console/user/{id}/mfa/reset"),
            String::new(),
        ),
        (
            "POST",
            "/v2/console/password_change".to_owned(),
            "{}".to_owned(),
        ),
        (
            "GET",
            "/v2/console/audit/log/users".to_owned(),
            String::new(),
        ),
        (
            "POST",
            format!("/v2/console/account/{id}/import"),
            "null".to_owned(),
        ),
        (
            "GET",
            format!("/v2/console/group/{id}/export"),
            String::new(),
        ),
        (
            "DELETE",
            format!("/v2/console/leaderboard/board-1/owner/{id}"),
            String::new(),
        ),
    ] {
        let reply = server.send(&method, &path, None, &body);
        assert_eq!(
            reply.refusal(UNAUTHENTICATED),
            "Console token required",
            "{method} {path}"
        );
    }
}
#[test]
pub(crate) fn a_player_cannot_reach_a_console_route() {
    let server = V2Server::start(v2_state());
    let reply = server.get("/v2/console/audit/log/users", Some(&player_token()));
    assert_eq!(reply.refusal(UNAUTHENTICATED), "Console token invalid");
}
#[test]
pub(crate) fn the_mfa_flag_routes_are_admin_only_and_then_reach_the_database() {
    let server = V2Server::start(v2_state());
    let maintainer = token_for(ConsoleRole::Maintainer);
    let admin = token_for(ConsoleRole::Admin);

    let require = format!("/v2/console/user/{NOBODY}/mfa/require");
    let reset = format!("/v2/console/user/{NOBODY}/mfa/reset");
    for (method, path) in [
        ("POST", require.clone()),
        ("DELETE", require.clone()),
        ("DELETE", reset),
    ] {
        let reply = server.send(method, &path, Some(&maintainer), "");
        assert_eq!(
            reply.refusal(PERMISSION_DENIED),
            "This console action requires the admin role.",
            "{method} {path}"
        );
        let reply = server.send(method, &path, Some(&admin), "");
        assert_eq!(reply.status, 503, "{method} {path}: {}", reply.body);
    }

    let reply = server.post("/v2/console/user/not-a-uuid/mfa/require", Some(&admin), "");
    assert_eq!(reply.refusal(INVALID_ARGUMENT), "Invalid console user id.");
}
#[test]
pub(crate) fn self_password_change_verifies_before_it_touches_the_database() {
    let server = V2Server::start(v2_state());
    let admin = token_for(ConsoleRole::Admin);

    let reply = server.post(
        "/v2/console/password_change",
        Some(&admin),
        r#"{"password":"short"}"#,
    );
    assert_eq!(
        reply.refusal(INVALID_ARGUMENT),
        "A console password must be at least 12 characters."
    );

    let reply = server.post(
        "/v2/console/password_change",
        Some(&admin),
        r#"{"current_password":"old","password":"a-long-enough-password"}"#,
    );
    assert_eq!(
        reply.refusal(FAILED_PRECONDITION),
        "The bootstrap console account has no password to change."
    );
}
#[test]
pub(crate) fn a_wallet_change_needs_a_player_token_and_currency_needs_the_console() {
    let server = V2Server::start(v2_state());

    let reply = server.send("PUT", "/v2/account/wallet", None, "{}");
    assert_eq!(
        reply.status, 401,
        "a wallet write needs a session: {}",
        reply.body
    );

    let reply = server.put("/v2/account/wallet", Some(&player_token()), "{}");
    assert_eq!(reply.status, 503, "{}", reply.body);
}
