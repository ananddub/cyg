//! Links that pin the gateway's answer order: body, then query, then session,
//! then the provider's own fields.

use crate::error::{INVALID_ARGUMENT, UNAUTHENTICATED};

use super::helpers::{V2Server, player_token, server_key, v2_state};

/// The body is decoded by the gateway before the security step, but the provider's
/// fields are read after it — so an unauthenticated request is answered about its
/// session even when its body would fail every field check.
#[test]
fn a_link_reads_the_session_between_the_body_and_the_provider_fields() {
    let server = V2Server::start(v2_state());

    // The body is read first: this 400 beats the missing credential.
    let malformed = server.post("/v2/account/link/custom", None, r#"{"id":"abc",}"#);
    assert_eq!(malformed.status, 400);
    assert!(
        malformed
            .refusal(INVALID_ARGUMENT)
            .starts_with("Invalid request body"),
        "{}",
        malformed.body
    );

    // A readable body that names nothing is still not authorised yet.
    assert_eq!(
        server
            .post("/v2/account/link/custom", None, r#"{"id":""}"#)
            .refusal(UNAUTHENTICATED),
        "Auth token required"
    );

    // With a session the same body is finally judged on its contents.
    let token = player_token();
    assert_eq!(
        server
            .post("/v2/account/link/custom", Some(&token), r#"{"id":""}"#)
            .refusal(INVALID_ARGUMENT),
        "Custom ID is required."
    );

    // A server key is not a session: these routes never accept the caller's half.
    assert_eq!(
        server
            .post(
                "/v2/account/link/custom",
                Some(&server_key()),
                r#"{"id":"abcdef"}"#
            )
            .refusal(UNAUTHENTICATED),
        "Auth token invalid"
    );
}

/// `LinkFacebook` is the one link whose `sync` arrives as a query parameter, and the
/// gateway populates query parameters after the body and before the credential — so a
/// bad one outranks a missing token, and a good one changes nothing observable.
#[test]
fn the_sync_flag_is_parsed_after_the_body_and_before_the_session() {
    let server = V2Server::start(v2_state());

    let bad = server.post(
        "/v2/account/link/facebook?sync=maybe",
        None,
        r#"{"token":"t"}"#,
    );
    assert_eq!(bad.status, 400);
    assert!(
        bad.refusal(INVALID_ARGUMENT)
            .contains(r#"cannot parse "maybe" as a boolean value"#),
        "{}",
        bad.body
    );

    // A malformed body still outranks it, because the body is read first.
    let both = server.post(
        "/v2/account/link/facebook?sync=maybe",
        Some(&player_token()),
        "{",
    );
    assert!(
        both.refusal(INVALID_ARGUMENT)
            .starts_with("Invalid request body"),
        "{}",
        both.body
    );

    // Every accepted spelling is simply carried: the friend import it selects is a
    // vendor question this route never reaches. Absent means the same thing.
    let token = player_token();
    for flag in ["true", "TRUE", "0", "t", "F", "1"] {
        let reply = server.post(
            &format!("/v2/account/link/facebook?sync={flag}"),
            Some(&token),
            r#"{"token":"t"}"#,
        );
        assert_eq!(
            reply.refusal(UNAUTHENTICATED),
            "Could not authenticate Facebook profile.",
            "sync={flag}"
        );
    }
    assert_eq!(
        server
            .post(
                "/v2/account/link/facebook",
                Some(&token),
                r#"{"token":"t"}"#
            )
            .refusal(UNAUTHENTICATED),
        "Could not authenticate Facebook profile."
    );
}
