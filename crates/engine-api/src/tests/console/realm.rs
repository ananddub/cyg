//! Who can sign in, what a refusal looks like, and why a token from one plane is
//! worth nothing on the other.

use crate::error::UNAUTHENTICATED;

use super::helpers::{ADMIN, PASSWORD, V2Server, login, player_token, v2_state};

#[test]
fn the_bootstrap_admin_can_log_in_and_reach_a_protected_route() {
    let server = V2Server::start(v2_state());
    let reply = server.post(
        "/v2/console/authenticate",
        None,
        &format!(r#"{{"username":"{ADMIN}","password":"{PASSWORD}"}}"#),
    );
    assert_eq!(reply.status, 200, "{}", reply.body);
    let body = reply.json();
    assert_eq!(body["username"], ADMIN);
    assert_eq!(body["role"], "admin");
    let token = body["token"].as_str().expect("token").to_owned();

    // No database is configured, so a request that got *past* auth answers 503:
    // that is precisely what proves the token was accepted.
    let reply = server.get("/v2/console/account", Some(&format!("Bearer {token}")));
    assert_eq!(reply.status, 503, "{}", reply.body);
}

#[test]
fn a_wrong_password_is_the_one_uniform_refusal() {
    let server = V2Server::start(v2_state());
    let reply = server.post(
        "/v2/console/authenticate",
        None,
        r#"{"username":"admin","password":"not-the-password"}"#,
    );
    assert_eq!(reply.refusal(UNAUTHENTICATED), "Invalid credentials.");
}

#[test]
fn an_empty_body_is_a_refusal_not_a_login() {
    let server = V2Server::start(v2_state());
    let reply = server.post("/v2/console/authenticate", None, "");
    assert_eq!(reply.refusal(UNAUTHENTICATED), "Invalid credentials.");
}

#[test]
fn a_protected_console_route_needs_a_console_token() {
    let server = V2Server::start(v2_state());
    let reply = server.get("/v2/console/account", None);
    assert_eq!(reply.refusal(UNAUTHENTICATED), "Console token required");
}

#[test]
fn a_console_token_is_not_a_player_session() {
    let server = V2Server::start(v2_state());
    let token = login(&server, ADMIN, PASSWORD);
    let reply = server.get("/v2/account", Some(&format!("Bearer {token}")));
    assert_eq!(reply.refusal(UNAUTHENTICATED), "Auth token invalid");
}

#[test]
fn a_player_session_is_not_a_console_token() {
    let server = V2Server::start(v2_state());
    let reply = server.get("/v2/console/account", Some(&player_token()));
    assert_eq!(reply.refusal(UNAUTHENTICATED), "Console token invalid");
}

#[test]
fn logout_revokes_exactly_the_token_it_was_given() {
    let server = V2Server::start(v2_state());
    let token = login(&server, ADMIN, PASSWORD);
    let auth = format!("Bearer {token}");

    assert_eq!(server.get("/v2/console/setting", Some(&auth)).status, 503);

    let reply = server.post("/v2/console/authenticate/logout", Some(&auth), "");
    assert_eq!(reply.status, 200, "{}", reply.body);

    let reply = server.get("/v2/console/setting", Some(&auth));
    assert_eq!(reply.refusal(UNAUTHENTICATED), "Console token invalid");

    // A fresh login is a different token, and is not caught by the revocation.
    let second = login(&server, ADMIN, PASSWORD);
    assert_eq!(
        server.get("/v2/console/setting", Some(&format!("Bearer {second}"))).status,
        503
    );
}
