use super::server::V2Server;
use super::{player_token, v2_state};
use crate::error::{INVALID_ARGUMENT, UNAUTHENTICATED, UNAVAILABLE};

const NO_DB: &str = "The database is not configured, set [control.session] database_endpoint.";

#[test]
fn an_unlink_needs_a_session_but_never_a_credential() {
    let server = V2Server::start(v2_state());

    let malformed = server.post("/v2/account/unlink/google", None, r#"{"token":}"#);
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
            .post("/v2/account/unlink/google", None, r#"{"token":"t"}"#)
            .refusal(UNAUTHENTICATED),
        "Auth token required"
    );

    let token = player_token();
    for path in [
        "apple",
        "custom",
        "email",
        "facebook",
        "facebookinstantgame",
        "gamecenter",
        "google",
        "steam",
    ] {
        assert_eq!(
            server
                .post(&format!("/v2/account/unlink/{path}"), Some(&token), "{}")
                .refusal(UNAVAILABLE),
            NO_DB,
            "/v2/account/unlink/{path}"
        );
    }
}

#[test]
fn a_supplied_vendor_credential_is_verified_and_a_blank_one_is_not_needed() {
    let server = V2Server::start(v2_state());
    let token = player_token();
    for (path, label) in [
        ("apple", "Apple"),
        ("facebook", "Facebook"),
        ("google", "Google"),
        ("steam", "Steam"),
    ] {
        let unlink =
            |body: &str| server.post(&format!("/v2/account/unlink/{path}"), Some(&token), body);
        assert_eq!(
            unlink(r#"{"token":""}"#).refusal(UNAVAILABLE),
            NO_DB,
            "{path} blank"
        );
        assert_eq!(
            unlink(r#"{"token":"t"}"#).refusal(UNAUTHENTICATED),
            format!("Could not authenticate {label} profile."),
            "{path} with a token"
        );
    }
}

#[test]
fn an_instant_game_unlink_clears_the_column_without_its_signature() {
    let server = V2Server::start(v2_state());
    let token = player_token();
    let unlink =
        |body: &str| server.post("/v2/account/unlink/facebookinstantgame", Some(&token), body);

    assert_eq!(
        unlink(r#"{"signed_player_info":""}"#).refusal(UNAVAILABLE),
        NO_DB
    );
    assert_eq!(
        unlink(r#"{"signedPlayerInfo":"x"}"#).refusal(UNAUTHENTICATED),
        "Could not authenticate Facebook Instant Game profile."
    );
}

#[test]
fn a_partial_game_center_bundle_is_ignored_rather_than_refused() {
    let server = V2Server::start(v2_state());
    let token = player_token();
    let unlink = |body: &str| server.post("/v2/account/unlink/gamecenter", Some(&token), body);

    let partial = r#"{"player_id":"p","bundle_id":"com.example.game","salt":"c2FsdA==","signature":"c2ln","timestamp_seconds":1700000000}"#;
    assert_eq!(unlink(partial).refusal(UNAVAILABLE), NO_DB);
    assert_eq!(
        unlink(r#"{"bundle_id":"com.example.game","timestamp_seconds":1700000000}"#)
            .refusal(UNAVAILABLE),
        NO_DB
    );
    let no_time = r#"{"bundle_id":"b","player_id":"p","public_key_url":"https://appleid.apple.com/auth/keys","salt":"s","signature":"g","timestamp_seconds":0}"#;
    assert_eq!(unlink(no_time).refusal(UNAVAILABLE), NO_DB);

    let complete = no_time.replace("\"timestamp_seconds\":0", "\"timestampSeconds\":1700000000");
    assert_eq!(
        unlink(&complete).refusal(UNAUTHENTICATED),
        "Could not authenticate GameCenter profile."
    );
}

#[test]
fn the_credentials_this_server_owns_are_compared_not_verified() {
    let server = V2Server::start(v2_state());
    let token = player_token();

    for (path, body) in [
        (
            "email",
            r#"{"email":"Player@Example.com","password":"12345678"}"#,
        ),
        ("email", "{}"),
        ("custom", r#"{"id":"abcdefgh"}"#),
        ("custom", "{}"),
    ] {
        assert_eq!(
            server
                .post(&format!("/v2/account/unlink/{path}"), Some(&token), body)
                .refusal(UNAVAILABLE),
            NO_DB,
            "{path} {body}"
        );
    }
}

#[test]
fn a_device_unlink_demands_the_id_it_deletes_a_row_by() {
    let server = V2Server::start(v2_state());
    let token = player_token();
    let unlink = |body: &str| server.post("/v2/account/unlink/device", Some(&token), body);

    assert_eq!(
        unlink("{}").refusal(INVALID_ARGUMENT),
        "A device ID must be supplied."
    );
    assert_eq!(
        unlink(r#"{"id":""}"#).refusal(INVALID_ARGUMENT),
        "A device ID must be supplied."
    );
    assert_eq!(
        unlink(r#"{"id":"0123456789ab"}"#).refusal(UNAVAILABLE),
        NO_DB
    );
}

#[test]
fn the_generic_unlink_deletes_rows_by_a_name_it_never_looks_up() {
    let server = V2Server::start(v2_state());
    let token = player_token();
    let unlink = |body: &str| server.post("/v2/account/unlink", Some(&token), body);

    assert_eq!(
        unlink("{}").refusal(INVALID_ARGUMENT),
        "A provider name must be supplied."
    );
    assert_eq!(
        unlink(r#"{"provider":"discord"}"#).refusal(UNAVAILABLE),
        NO_DB
    );
    assert_eq!(
        unlink(r#"{"provider":"Discord","payload":{"a":1}}"#).refusal(UNAVAILABLE),
        NO_DB
    );
}
