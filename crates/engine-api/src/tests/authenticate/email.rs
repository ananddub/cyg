use super::helpers::{V2Server, server_key, v2_state};
use crate::error::{INVALID_ARGUMENT, UNAVAILABLE};

const PASSWORD: &str = "12345678";

#[test]
fn email_authentication_branches_on_whether_an_address_arrived() {
    let server = V2Server::start(v2_state());
    let key = server_key();
    let email = |body: &str, query: &str| {
        server.post(
            &format!("/v2/account/authenticate/email{query}"),
            Some(&key),
            body,
        )
    };

    assert_eq!(
        email("null", "").refusal(INVALID_ARGUMENT),
        "Email address and password is required."
    );
    assert_eq!(
        email(r#"{"email":"","password":""}"#, "").refusal(INVALID_ARGUMENT),
        "Password must be at least 8 characters long."
    );
    assert_eq!(
        email(&format!(r#"{{"email":"","password":"{PASSWORD}"}}"#), "").refusal(INVALID_ARGUMENT),
        "Username is required when email address is not supplied."
    );
    assert_eq!(
        email(r#"{"email":"a@b.co","password":"12345678"}"#, "").refusal(INVALID_ARGUMENT),
        "Invalid email address, must be 10-255 bytes."
    );
    assert_eq!(
        email(r#"{"email":"no-at-sign-here","password":"12345678"}"#, "").refusal(INVALID_ARGUMENT),
        "Invalid email address format."
    );
    assert_eq!(
        email(
            r#"{"email":"sp ace@example.com","password":"12345678"}"#,
            ""
        )
        .refusal(INVALID_ARGUMENT),
        "Invalid email address, no spaces or control characters allowed."
    );

    assert_eq!(
        email(
            &format!(r#"{{"email":"player@example.com","password":"{PASSWORD}"}}"#),
            ""
        )
        .refusal(UNAVAILABLE)
        .as_str(),
        "The database is not configured, set [control.session] database_endpoint."
    );
    assert_eq!(
        email(
            &format!(r#"{{"email":"","password":"{PASSWORD}"}}"#),
            "?username=player"
        )
        .refusal(UNAVAILABLE),
        "The database is not configured, set [control.session] database_endpoint."
    );
}
