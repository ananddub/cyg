//! The vendor-parity additions to `/v2/iap`: Facebook Instant and Samsung
//! purchase recording and the by-product subscription read. Each answers in
//! the gateway's order — credential, then body, then the missing database —
//! so a well-formed request on this plane is stopped by the absence, not by
//! the route.

use super::server::V2Server;
use super::{player_token, v2_state};
use crate::error::{INVALID_ARGUMENT, UNAUTHENTICATED, UNAVAILABLE};

const NO_DB: &str = "The database is not configured, set [control.session] database_url.";

#[test]
fn the_new_vendor_routes_demand_a_player_token_in_the_shared_envelope() {
    let server = V2Server::start(v2_state());

    for path in [
        "/v2/iap/purchase/facebookinstant",
        "/v2/iap/purchase/samsung",
    ] {
        let reply = server.post(path, None, "{}");
        assert_eq!(reply.refusal(UNAUTHENTICATED), "Auth token required", "{path}");
        let body = reply.json();
        assert_eq!(body["error"]["code"].as_u64(), Some(16));
        assert!(body["error"]["details"].is_array());
    }

    let reply = server.get("/v2/iap/subscription/season-pass", None);
    assert_eq!(reply.refusal(UNAUTHENTICATED), "Auth token required");
}

#[test]
fn a_vendor_purchase_without_a_credential_is_refused() {
    let server = V2Server::start(v2_state());
    let auth = player_token();

    for path in [
        "/v2/iap/purchase/facebookinstant",
        "/v2/iap/purchase/samsung",
    ] {
        let reply = server.post(path, Some(&auth), "{}");
        assert_eq!(
            reply.refusal(INVALID_ARGUMENT),
            "purchase token cannot be empty",
            "{path}"
        );
    }
}

#[test]
fn a_well_formed_vendor_purchase_reaches_the_database() {
    let server = V2Server::start(v2_state());
    let auth = player_token();

    // Facebook Instant hands a signed request, Samsung a purchase id: both
    // aliases of the shared token envelope pass validation and reach the write.
    let reply = server.post(
        "/v2/iap/purchase/facebookinstant",
        Some(&auth),
        r#"{"signed_request":"payload.sig"}"#,
    );
    assert_eq!(reply.refusal(UNAVAILABLE), NO_DB);

    let reply = server.post(
        "/v2/iap/purchase/samsung",
        Some(&auth),
        r#"{"purchase_id":"PA1R2000000000000"}"#,
    );
    assert_eq!(reply.refusal(UNAVAILABLE), NO_DB);

    let reply = server.get("/v2/iap/subscription/season-pass", Some(&auth));
    assert_eq!(reply.refusal(UNAVAILABLE), NO_DB);
}
