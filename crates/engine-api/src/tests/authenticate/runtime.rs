//! The runtime-provider `Authenticate` route.

use super::helpers::{V2Server, server_key, v2_state};
use crate::error::{INVALID_ARGUMENT, NOT_FOUND};

/// A runtime provider is looked up by the lowercase name it was registered under, so
/// the refusal echoes that spelling.
#[test]
fn the_runtime_provider_route_names_the_provider_it_could_not_find() {
    let server = V2Server::start(v2_state());
    let key = server_key();
    assert_eq!(
        server
            .post("/v2/account/authenticate", Some(&key), r#"{"provider":""}"#)
            .refusal(INVALID_ARGUMENT),
        "Provider name is required."
    );
    assert_eq!(
        server
            .post(
                "/v2/account/authenticate",
                Some(&key),
                r#"{"provider":"Discord"}"#
            )
            .refusal(NOT_FOUND),
        "Authentication provider not found: discord"
    );
}
