//! The `Link*` field ladders: how a device, custom, email or Game Center
//! credential is judged, and in which order.

use crate::error::{INVALID_ARGUMENT, UNAUTHENTICATED, UNAVAILABLE};

use super::helpers::{NO_DB, V2Server, first_fields, player_token, server_key, v2_state};

/// Device links use the device id rules, so spaces are judged before length and an
/// id that breaks both answers the space.
#[test]
fn a_linked_device_id_is_demanded_by_the_device_rules() {
    let server = V2Server::start(v2_state());
    let token = player_token();
    let link = |body: &str| server.post("/v2/account/link/device", Some(&token), body);

    assert_eq!(
        link(r#"{"id":""}"#).refusal(INVALID_ARGUMENT),
        "Device ID is required."
    );
    assert_eq!(
        link(r#"{"id":"a b"}"#).refusal(INVALID_ARGUMENT),
        "Device ID invalid, no spaces or control characters allowed."
    );
    assert_eq!(
        link(r#"{"id":"short"}"#).refusal(INVALID_ARGUMENT),
        "Device ID invalid, must be 10-128 bytes."
    );
    assert_eq!(link(r#"{"id":"0123456789ab"}"#).refusal(UNAVAILABLE), NO_DB);
}

/// The same three rules as the authentication route, but the link's own wording: a
/// link complains "Invalid custom ID …" where authentication says "Custom ID invalid …".
#[test]
fn a_linked_custom_id_keeps_the_rules_and_changes_the_wording() {
    let server = V2Server::start(v2_state());
    let token = player_token();
    let link = |body: &str| server.post("/v2/account/link/custom", Some(&token), body);

    assert_eq!(
        link(r#"{"id":""}"#).refusal(INVALID_ARGUMENT),
        "Custom ID is required."
    );
    assert_eq!(
        link(r#"{"id":"a b"}"#).refusal(INVALID_ARGUMENT),
        "Invalid custom ID, no spaces or control characters allowed."
    );
    assert_eq!(
        server
            .post(
                "/v2/account/authenticate/custom",
                Some(&server_key()),
                r#"{"id":"a b"}"#
            )
            .refusal(INVALID_ARGUMENT),
        "Custom ID invalid, no spaces or control characters allowed."
    );

    assert_eq!(
        link(r#"{"id":"ab"}"#).refusal(INVALID_ARGUMENT),
        "Invalid custom ID, must be 6-128 bytes."
    );
    assert_eq!(link(r#"{"id":"abcdefgh"}"#).refusal(UNAVAILABLE), NO_DB);
}

/// The link's email ladder is the authentication one reordered: both halves must
/// arrive, then the address is scanned for spaces, then the password length is judged,
/// and only then the format and byte length that authentication looks at first.
#[test]
fn a_linked_email_judges_the_password_before_the_address() {
    let server = V2Server::start(v2_state());
    let token = player_token();
    let link = |body: &str| server.post("/v2/account/link/email", Some(&token), body);

    assert_eq!(
        link(r#"{"email":"","password":""}"#).refusal(INVALID_ARGUMENT),
        "Email address and password is required."
    );
    // Half a credential is no credential — and unlike authentication, an empty address
    // here is not a username login.
    assert_eq!(
        link(r#"{"email":"player@example.com"}"#).refusal(INVALID_ARGUMENT),
        "Email address and password is required."
    );
    assert_eq!(
        link(r#"{"email":"sp ace@example.com","password":"12345678"}"#).refusal(INVALID_ARGUMENT),
        "Invalid email address, no spaces or control characters allowed."
    );

    // One body, two routes, two answers: that is the ordering difference itself.
    let both = r#"{"email":"no-at-sign","password":"123"}"#;
    assert_eq!(
        server
            .post("/v2/account/authenticate/email", Some(&server_key()), both)
            .refusal(INVALID_ARGUMENT),
        "Invalid email address format."
    );
    assert_eq!(
        link(both).refusal(INVALID_ARGUMENT),
        "Password must be at least 8 characters long."
    );

    assert_eq!(
        link(r#"{"email":"no-at-sign-here","password":"12345678"}"#).refusal(INVALID_ARGUMENT),
        "Invalid email address format."
    );
    assert_eq!(
        link(r#"{"email":"a@b.co","password":"12345678"}"#).refusal(INVALID_ARGUMENT),
        "Invalid email address, must be 10-255 bytes."
    );
    // The address is folded before it is written, so mixed case is accepted, not refused.
    assert_eq!(
        link(r#"{"email":"Player@Example.COM","password":"12345678"}"#).refusal(UNAVAILABLE),
        NO_DB
    );
}

/// The Game Center bundle is demanded in the order `CheckGameCenterID` receives it,
/// timestamp last, and a complete one then reaches for the same profile check the
/// authentication route fails.
#[test]
fn a_linked_game_center_bundle_is_demanded_field_by_field() {
    let server = V2Server::start(v2_state());
    let token = player_token();
    let path = "/v2/account/link/gamecenter";
    let bundle = [
        ("bundle_id", "\"com.example.game\""),
        ("player_id", "\"player-1\""),
        ("public_key_url", "\"https://appleid.apple.com/auth/keys\""),
        ("salt", "\"c2FsdA==\""),
        ("signature", "\"c2ln\""),
    ];
    let expected = [
        "GameCenter bundle ID is required.",
        "GameCenter player ID is required.",
        "GameCenter public key URL is required.",
        "GameCenter salt is required.",
        "GameCenter signature is required.",
        "GameCenter timestamp is required.",
    ];

    // A link body is not wrapped, so there is no "credentials are required" step:
    // the empty body is simply the first missing field.
    for step in 0..bundle.len() {
        let body = format!("{{{}}}", first_fields(bundle, step));
        assert_eq!(
            server
                .post(path, Some(&token), &body)
                .refusal(INVALID_ARGUMENT),
            expected[step],
            "at {step}: {body}"
        );
    }
    let five = first_fields(bundle, bundle.len());
    assert_eq!(
        server
            .post(path, Some(&token), &format!("{{{five}}}"))
            .refusal(INVALID_ARGUMENT),
        expected[5]
    );
    // Camel-case spellings of the same bundle are the same request.
    assert_eq!(
        server
            .post(
                path,
                Some(&token),
                &format!("{{{five},\"timestampSeconds\":1700000000}}")
            )
            .refusal(UNAUTHENTICATED),
        "Could not authenticate GameCenter profile."
    );
}
