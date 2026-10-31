use super::helpers::{V2Server, first_fields, server_key, v2_state};
use crate::error::{INVALID_ARGUMENT, UNAUTHENTICATED};

#[test]
fn a_game_center_bundle_is_demanded_field_by_field() {
    let server = V2Server::start(v2_state());
    let key = server_key();
    let path = "/v2/account/authenticate/gamecenter";

    assert_eq!(
        server
            .post(path, Some(&key), "null")
            .refusal(INVALID_ARGUMENT),
        "GameCenter access credentials are required."
    );

    let bundle = [
        ("bundle_id", "\"b\""),
        ("player_id", "\"p\""),
        ("public_key_url", "\"https://example.com/key\""),
        ("salt", "\"s\""),
        ("signature", "\"g\""),
    ];
    for step in 0..bundle.len() {
        let body = format!("{{{}}}", first_fields(bundle, step));
        let expected = [
            "GameCenter bundle ID is required.",
            "GameCenter player ID is required.",
            "GameCenter public key URL is required.",
            "GameCenter salt is required.",
            "GameCenter signature is required.",
            "GameCenter timestamp is required.",
        ][step];
        assert_eq!(
            server
                .post(path, Some(&key), &body)
                .refusal(INVALID_ARGUMENT),
            expected,
            "at {step}"
        );
    }

    let complete = format!(
        "{{{},\"timestamp_seconds\":1700000000}}",
        first_fields(bundle, bundle.len())
    );
    assert_eq!(
        server
            .post(path, Some(&key), &complete)
            .refusal(UNAUTHENTICATED),
        "Could not authenticate GameCenter profile."
    );
}
