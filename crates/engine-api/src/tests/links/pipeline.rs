use crate::error::{INVALID_ARGUMENT, UNAUTHENTICATED};

use super::helpers::{V2Server, player_token, server_key, v2_state};

#[test]
fn a_link_reads_the_session_between_the_body_and_the_provider_fields() {
    let server = V2Server::start(v2_state());

    let malformed = server.post("/v2/account/link/custom", None, r#"{"id":"abc",}"#);
    assert_eq!(malformed.status, 400);
    assert!(
        malformed
            .refusal(INVALID_ARGUMENT)
            .starts_with("Invalid request body"),
        "{}",
        malformed.body
    );

    assert_eq!(
        server
            .post("/v2/account/link/custom", None, r#"{"id":""}"#)
            .refusal(UNAUTHENTICATED),
        "Auth token required"
    );

    let token = player_token();
    assert_eq!(
        server
            .post("/v2/account/link/custom", Some(&token), r#"{"id":""}"#)
            .refusal(INVALID_ARGUMENT),
        "Custom ID is required."
    );

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
