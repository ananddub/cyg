use super::helpers::{V2Server, server_key, v2_state};
use crate::error::{DATA_LOSS, FAILED_PRECONDITION, INVALID_ARGUMENT, UNAUTHENTICATED};

#[test]
fn apple_and_steam_are_refused_by_the_configuration_they_lack() {
    let server = V2Server::start(v2_state());
    let key = server_key();
    let empty = server.post("/v2/account/authenticate/apple", Some(&key), "{}");
    assert_eq!(empty.status, 400);
    assert_eq!(
        empty.refusal(FAILED_PRECONDITION),
        "Apple authentication is not configured."
    );

    let token = server.post(
        "/v2/account/authenticate/steam",
        Some(&key),
        r#"{"token":"anything"}"#,
    );
    assert_eq!(
        token.refusal(FAILED_PRECONDITION),
        "Steam authentication is not configured."
    );
}

#[test]
fn facebook_and_google_stop_at_the_profile_they_cannot_read() {
    let server = V2Server::start(v2_state());
    let key = server_key();
    for (profile, body) in [
        ("Facebook", r#"{"token":""}"#),
        ("Google", r#"{"token":""}"#),
    ] {
        let missing = server.post(
            &format!("/v2/account/authenticate/{}", profile.to_lowercase()),
            Some(&key),
            body,
        );
        assert_eq!(
            missing.refusal(INVALID_ARGUMENT),
            format!("{profile} access token is required.")
        );
    }

    let with_token = server.post(
        "/v2/account/authenticate/facebook",
        Some(&key),
        r#"{"token":"t"}"#,
    );
    assert_eq!(
        with_token.refusal(UNAUTHENTICATED),
        "Could not authenticate Facebook profile."
    );

    let hostile_username = server.post(
        "/v2/account/authenticate/google?username=bad%5B%5Dname",
        Some(&key),
        r#"{"token":"t"}"#,
    );
    assert_eq!(
        hostile_username.refusal(INVALID_ARGUMENT),
        "Username invalid, no spaces or control characters allowed."
    );
}

#[test]
fn an_instant_game_signature_is_data_loss_not_a_rejected_credential() {
    let server = V2Server::start(v2_state());
    let key = server_key();
    let missing = server.post(
        "/v2/account/authenticate/facebookinstantgame",
        Some(&key),
        "{}",
    );
    assert_eq!(
        missing.refusal(INVALID_ARGUMENT),
        "Signed Player Info for a Facebook Instant Game is required."
    );

    for field in ["signed_player_info", "signedPlayerInfo"] {
        let signed = server.post(
            "/v2/account/authenticate/facebookinstantgame",
            Some(&key),
            &format!(r#"{{"{field}":"x"}}"#),
        );
        assert_eq!(
            signed.refusal(DATA_LOSS),
            "Error extracting the Facebook Instant Game player ID or validating the signature"
        );
    }
}
