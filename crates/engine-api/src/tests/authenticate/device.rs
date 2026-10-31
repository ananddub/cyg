use super::helpers::{V2Server, server_key, v2_state};
use crate::error::{INVALID_ARGUMENT, UNAVAILABLE};

const DEVICE_ID: &str = "0123456789ab";

#[test]
fn the_device_id_is_validated_before_the_username_it_might_create() {
    let server = V2Server::start(v2_state());
    let key = server_key();
    let authenticate = |body: &str, query: &str| {
        server.post(
            &format!("/v2/account/authenticate/device{query}"),
            Some(&key),
            body,
        )
    };

    assert_eq!(
        authenticate(r#"{"id":""}"#, "").refusal(INVALID_ARGUMENT),
        "Device ID is required."
    );
    assert_eq!(
        authenticate(r#"{"id":"01234567 89"}"#, "").refusal(INVALID_ARGUMENT),
        "Device ID invalid, no spaces or control characters allowed."
    );
    assert_eq!(
        authenticate(r#"{"id":"short"}"#, "?username=bad%5B%5Dname").refusal(INVALID_ARGUMENT),
        "Device ID invalid, must be 10-128 bytes."
    );

    assert_eq!(
        authenticate(
            &format!(r#"{{"id":"{DEVICE_ID}"}}"#),
            "?username=bad%5B%5Dname"
        )
        .refusal(INVALID_ARGUMENT),
        "Username invalid, no spaces or control characters allowed."
    );

    assert_eq!(
        authenticate(&format!(r#"{{"id":"{DEVICE_ID}"}}"#), "").refusal(UNAVAILABLE),
        "The database is not configured, set [control.session] database_endpoint."
    );
}
