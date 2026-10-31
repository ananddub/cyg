use super::server::V2Server;
use super::{server_key, v2_state};
use crate::error::{INVALID_ARGUMENT, UNAUTHENTICATED, UNAVAILABLE};

#[test]
fn the_body_is_decoded_before_the_credential_is_read() {
    let server = V2Server::start(v2_state());

    let broken = server.post("/v2/account/authenticate/device", None, r#"{"id":"#);
    assert_eq!(broken.status, 400);
    assert!(
        broken
            .refusal(INVALID_ARGUMENT)
            .starts_with("Invalid request body"),
        "a decode failure names itself the way the gateway names it"
    );

    let anonymous = server.post(
        "/v2/account/authenticate/device",
        None,
        r#"{"id":"0123456789ab"}"#,
    );
    assert_eq!(anonymous.status, 401);
    assert_eq!(anonymous.refusal(UNAUTHENTICATED), "Server key required");
    assert_eq!(
        anonymous.header("www-authenticate"),
        Some("Bearer realm=\"cyg\"")
    );
    assert_eq!(anonymous.header("content-type"), Some("application/json"));
}

#[test]
fn a_bad_query_flag_is_refused_before_the_credential_too() {
    let server = V2Server::start(v2_state());
    let reply = server.post("/v2/account/authenticate/device?create=maybe", None, "{}");
    assert_eq!(reply.status, 400);
    let message = reply.refusal(INVALID_ARGUMENT);
    assert!(
        message.contains("cannot parse \"maybe\" as a boolean value"),
        "{message}"
    );
    assert!(message.contains("TRUE || FALSE"), "{message}");
}

#[test]
fn the_server_key_is_the_user_half_of_basic_and_the_colon_is_part_of_it() {
    let server = V2Server::start(v2_state());

    let accepted = server.post(
        "/v2/account/authenticate/device",
        Some(&server_key()),
        r#"{"id":"0123456789ab"}"#,
    );
    assert_eq!(accepted.status, 503);
    assert_eq!(
        accepted.refusal(UNAVAILABLE),
        "The database is not configured, set [control.session] database_endpoint."
    );

    let colonless = server.post(
        "/v2/account/authenticate/device",
        Some("Basic ZGVmYXVsdGtleQ=="),
        "{}",
    );
    assert_eq!(colonless.refusal(UNAUTHENTICATED), "Server key invalid");

    let wrong = server.post(
        "/v2/account/authenticate/device",
        Some("Basic d3Jvbmc6"),
        "{}",
    );
    assert_eq!(wrong.refusal(UNAUTHENTICATED), "Server key invalid");
}

#[test]
fn an_account_route_needs_a_session_and_will_not_take_the_server_key() {
    let server = V2Server::start(v2_state());

    let no_credential = server.post("/v2/account/link/custom", None, r#"{"id":"abcdefgh"}"#);
    assert_eq!(no_credential.status, 401);
    assert_eq!(
        no_credential.refusal(UNAUTHENTICATED),
        "Auth token required"
    );

    let wrong_kind = server.post("/v2/account/unlink/custom", Some(&server_key()), "{}");
    assert_eq!(wrong_kind.refusal(UNAUTHENTICATED), "Auth token invalid");

    let undecodable = server.post(
        "/v2/account/unlink/custom",
        Some("Bearer not.a.token"),
        "{}",
    );
    assert_eq!(undecodable.refusal(UNAUTHENTICATED), "Auth token invalid");
}
