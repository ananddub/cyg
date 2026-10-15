//! The order the gateway imposes on every `/v2` request, and the shape it imposes
//! on every refusal: the body and query string are decoded *before* anyone asks who
//! the caller is, and a refusal is a JSON object carrying the message, never the web
//! framework's plain text.

use super::server::V2Server;
use super::{server_key, v2_state};
use crate::error::{INVALID_ARGUMENT, UNAUTHENTICATED, UNAVAILABLE};

/// A request that is both malformed and unauthenticated answers 400 — the one place
/// a caller learns something without proving who it is. A well-formed one answers 401.
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
        Some("Bearer realm=\"openoxide\"")
    );
    assert_eq!(anonymous.header("content-type"), Some("application/json"));
}

/// The query string is read by that same step, so a bad `create` is refused before
/// the credential — with Go's `strconv.ParseBool` wording, down to its spellings.
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

/// `Basic` is only the transport: the user half is the key, the colon separates it
/// from the unused password, and both mistakes answer the same way.
#[test]
fn the_server_key_is_the_user_half_of_basic_and_the_colon_is_part_of_it() {
    let server = V2Server::start(v2_state());

    // The real key clears the credential step and is stopped by the missing database.
    let accepted = server.post(
        "/v2/account/authenticate/device",
        Some(&server_key()),
        r#"{"id":"0123456789ab"}"#,
    );
    assert_eq!(accepted.status, 503);
    assert_eq!(
        accepted.refusal(UNAVAILABLE),
        "The database is not configured, set [control.session] database_url."
    );

    // `base64("defaultkey")` — no colon, so there is no user to read.
    let colonless = server.post(
        "/v2/account/authenticate/device",
        Some("Basic ZGVmYXVsdGtleQ=="),
        "{}",
    );
    assert_eq!(colonless.refusal(UNAUTHENTICATED), "Server key invalid");

    // `base64("wrong:")` — well formed, and not the key.
    let wrong = server.post(
        "/v2/account/authenticate/device",
        Some("Basic d3Jvbmc6"),
        "{}",
    );
    assert_eq!(wrong.refusal(UNAUTHENTICATED), "Server key invalid");
}

/// Account routes need a session, and the key that authenticates a player cannot
/// stand in. A `Basic` credential is answered as an invalid token, not a missing one.
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
