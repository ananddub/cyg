use crate::error::{FAILED_PRECONDITION, INVALID_ARGUMENT, NOT_FOUND, UNAUTHENTICATED};

use super::helpers::{V2Server, player_token, server_key, v2_state};

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
    assert_eq!(
        server
            .post("/v2/account/link/apple", None, "{}")
            .refusal(UNAUTHENTICATED),
        "Auth token required"
    );
}

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
    assert_eq!(
        server
            .post("/v2/account/authenticate", Some(&server_key()), "{}")
            .refusal(INVALID_ARGUMENT),
        "Provider name is required."
    );
}
