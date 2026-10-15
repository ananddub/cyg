//! The vendor-specific `Link*` routes: how each reads its credential and where the
//! server stops short of checking it.

use crate::error::{FAILED_PRECONDITION, INVALID_ARGUMENT, NOT_FOUND, UNAUTHENTICATED};

use super::helpers::{V2Server, player_token, server_key, v2_state};

/// `LinkSteam` is the one link whose body wraps the account, because the same request
/// carries a `sync` flag. A flat token is an unknown member of that body and drops,
/// and the API layer refuses before the configuration gate that authentication hits.
#[test]
fn the_steam_link_reads_its_token_from_inside_the_wrapped_account() {
    let server = V2Server::start(v2_state());
    let token = player_token();
    let link = |body: &str| server.post("/v2/account/link/steam", Some(&token), body);

    assert_eq!(
        link(r#"{"token":"t"}"#).refusal(INVALID_ARGUMENT),
        "Steam access token is required."
    );
    assert_eq!(
        link(r#"{"account":{"token":""}}"#).refusal(INVALID_ARGUMENT),
        "Steam access token is required."
    );
    assert_eq!(
        link(r#"{"account":{"token":"t"},"sync":false}"#).refusal(FAILED_PRECONDITION),
        "Steam authentication is not configured."
    );
}

/// Apple is refused by the configuration it has no `[social.apple]` for, before its
/// token is looked at — so the message about a missing Apple token is unreachable here.
#[test]
fn the_apple_link_is_refused_by_its_configuration_not_by_its_token() {
    let server = V2Server::start(v2_state());
    let token = player_token();
    for body in [r#"{"token":""}"#, r#"{"token":"whatever","id":"subject"}"#] {
        assert_eq!(
            server
                .post("/v2/account/link/apple", Some(&token), body)
                .refusal(FAILED_PRECONDITION),
            "Apple authentication is not configured.",
            "{body}"
        );
    }
    // The session step still comes first: anonymous is not authorised, not unconfigured.
    assert_eq!(
        server
            .post("/v2/account/link/apple", None, "{}")
            .refusal(UNAUTHENTICATED),
        "Auth token required"
    );
}

/// Facebook and Google do examine the credential they are given, and stop at the
/// profile this server has no way to check.
#[test]
fn facebook_and_google_ask_for_a_token_then_for_something_to_check_it_against() {
    let server = V2Server::start(v2_state());
    let token = player_token();
    for (path, label) in [("facebook", "Facebook"), ("google", "Google")] {
        let link =
            |body: &str| server.post(&format!("/v2/account/link/{path}"), Some(&token), body);
        assert_eq!(
            link(r#"{"token":""}"#).refusal(INVALID_ARGUMENT),
            format!("{label} access token is required.")
        );
        assert_eq!(
            link("{}").refusal(INVALID_ARGUMENT),
            format!("{label} access token is required.")
        );
        assert_eq!(
            link(r#"{"token":"t"}"#).refusal(UNAUTHENTICATED),
            format!("Could not authenticate {label} profile.")
        );
    }
}

/// An Instant Game signature is verified by an HMAC the app secret keys, and an
/// unverifiable one is reported here as `Unauthenticated` — the spelling the *unlink*
/// of the same profile uses, where authenticating it calls the same failure `DataLoss`.
#[test]
fn an_instant_game_link_demands_a_signature_it_cannot_verify() {
    let server = V2Server::start(v2_state());
    let token = player_token();
    let link = |body: &str| server.post("/v2/account/link/facebookinstantgame", Some(&token), body);

    for absent in ["{}", "null"] {
        assert_eq!(
            link(absent).refusal(INVALID_ARGUMENT),
            "Signed Player Info for a Facebook Instant Game is required.",
            "{absent}"
        );
    }
    for field in ["signed_player_info", "signedPlayerInfo"] {
        assert_eq!(
            link(&format!(r#"{{"{field}":"x"}}"#)).refusal(UNAUTHENTICATED),
            "Could not authenticate Facebook Instant Game profile."
        );
    }
}

/// The runtime provider link has no field checks of its own: the registry lookup is
/// what refuses, so it answers with the name it was handed, folded to lowercase — and
/// an empty name is that same refusal, not a separate complaint.
#[test]
fn the_runtime_provider_link_answers_with_the_name_it_was_given() {
    let server = V2Server::start(v2_state());
    let token = player_token();
    let link = |body: &str| server.post("/v2/account/link", Some(&token), body);

    assert_eq!(
        link(r#"{"provider":"Discord"}"#).refusal(NOT_FOUND),
        "Authentication provider not found: discord"
    );
    assert_eq!(
        link(r#"{"provider":"DISCORD"}"#).refusal(NOT_FOUND),
        "Authentication provider not found: discord"
    );
    assert_eq!(
        link("{}").refusal(NOT_FOUND),
        "Authentication provider not found: "
    );
    // Authentication, by contrast, demands the name before it looks anything up.
    assert_eq!(
        server
            .post("/v2/account/authenticate", Some(&server_key()), "{}")
            .refusal(INVALID_ARGUMENT),
        "Provider name is required."
    );
}
