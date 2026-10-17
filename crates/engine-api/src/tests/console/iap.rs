//! The console IAP additions: vendor validation, vendor config reset and the
//! product-level subscription delete — gated by the token and the role, then
//! answered ahead of a database that is not there, except the unpersisted
//! validation, which needs no database at all.

use uuid::Uuid;

use crate::error::{INVALID_ARGUMENT, PERMISSION_DENIED, UNAUTHENTICATED, UNAVAILABLE};
use crate::infra::console::ConsoleRole;

use super::helpers::{V2Server, token_for, v2_state};

const NO_DB: &str = "The database is not configured, set [control.session] database_url.";
const READ_ONLY: &str = "This console user has read-only access.";

/// A valid credential body for the apple validation route.
const RECEIPT: &str = r#"{"receipt":"MIIB…base64-pkcs7"}"#;

#[test]
fn the_iap_writes_demand_a_console_token_in_the_shared_envelope() {
    let server = V2Server::start(v2_state());

    let reply = server.post("/v2/console/iap/apple/validate", None, RECEIPT);
    assert_eq!(reply.refusal(UNAUTHENTICATED), "Console token required");
    let body = reply.json();
    assert_eq!(body["error"]["code"].as_u64(), Some(16));
    assert!(body["error"]["details"].is_array());

    let reply = server.delete("/v2/console/iap/google/reset", None);
    assert_eq!(reply.refusal(UNAUTHENTICATED), "Console token required");

    let reply = server.delete("/v2/console/subscription/season-pass", None);
    assert_eq!(reply.refusal(UNAUTHENTICATED), "Console token required");
}

#[test]
fn a_read_only_operator_cannot_validate_reset_or_delete_iap_rows() {
    let server = V2Server::start(v2_state());
    let auth = token_for(ConsoleRole::Readonly);

    let reply = server.post("/v2/console/iap/apple/validate", Some(&auth), RECEIPT);
    assert_eq!(reply.refusal(PERMISSION_DENIED), READ_ONLY);

    let reply = server.delete("/v2/console/iap/huawei/reset", Some(&auth));
    assert_eq!(reply.refusal(PERMISSION_DENIED), READ_ONLY);

    let reply = server.delete("/v2/console/subscription/season-pass", Some(&auth));
    assert_eq!(reply.refusal(PERMISSION_DENIED), READ_ONLY);
}

#[test]
fn an_unknown_vendor_is_refused_before_anything_else_about_the_request() {
    let server = V2Server::start(v2_state());
    let auth = token_for(ConsoleRole::Admin);

    let reply = server.post("/v2/console/iap/nintendo/validate", Some(&auth), RECEIPT);
    assert_eq!(reply.refusal(INVALID_ARGUMENT), "Unknown IAP vendor: nintendo");

    let reply = server.delete("/v2/console/iap/nintendo/reset", Some(&auth));
    assert_eq!(reply.refusal(INVALID_ARGUMENT), "Unknown IAP vendor: nintendo");
}

#[test]
fn an_unpersisted_validation_returns_the_parsed_purchase_with_no_database() {
    let server = V2Server::start(v2_state());
    let auth = token_for(ConsoleRole::Admin);

    let reply = server.post(
        "/v2/console/iap/apple/validate",
        Some(&auth),
        r#"{"receipt":"MIIB…base64-pkcs7","persist":false}"#,
    );
    assert_eq!(reply.status, 200, "{}", reply.body);
    let body = reply.json();
    // The reference server's `ValidatePurchaseResponse` shape: a list, one entry.
    let purchases = body["validated_purchases"]
        .as_array()
        .expect("validated purchases list");
    assert_eq!(purchases.len(), 1);
    let id = purchases[0]["transaction_id"].as_str().expect("a transaction id");
    assert_eq!(id.len(), 32, "the row key is the credential's MD5 digest");
    assert_eq!(purchases[0]["environment"].as_str(), Some("UNKNOWN"));

    // The subscription kind answers the single-entity shape of its mirror.
    let reply = server.post(
        "/v2/console/iap/google/validate",
        Some(&auth),
        r#"{"purchase_token":"g gp..token","kind":"subscription","persist":false}"#,
    );
    assert_eq!(reply.status, 200, "{}", reply.body);
    let body = reply.json();
    assert!(body["subscription"]["original_transaction_id"].is_string());
    assert_eq!(body["subscription"]["state"].as_i64(), Some(1));
}

#[test]
fn a_validation_that_would_write_is_refused_or_stopped_by_the_missing_database() {
    let server = V2Server::start(v2_state());
    let auth = token_for(ConsoleRole::Admin);

    // Facebook Instant has no subscriptions — in the reference server or here.
    let reply = server.post(
        "/v2/console/iap/facebookinstant/validate",
        Some(&auth),
        r#"{"signed_request":"payload.sig","kind":"subscription"}"#,
    );
    assert_eq!(
        reply.refusal(INVALID_ARGUMENT),
        "Facebook Instant subscriptions are not supported."
    );

    let reply = server.post(
        "/v2/console/iap/samsung/validate",
        Some(&auth),
        r#"{"kind":"voucher"}"#,
    );
    assert_eq!(
        reply.refusal(INVALID_ARGUMENT),
        "Validation kind must be \"purchase\" or \"subscription\"."
    );

    let reply = server.post("/v2/console/iap/apple/validate", Some(&auth), "{}");
    assert_eq!(
        reply.refusal(INVALID_ARGUMENT),
        "receipt or purchase token is required"
    );

    // Persisting (the default) needs an account to attach the row to…
    let reply = server.post("/v2/console/iap/apple/validate", Some(&auth), RECEIPT);
    assert_eq!(
        reply.refusal(INVALID_ARGUMENT),
        "Persisting a validation needs a user id."
    );
    let reply = server.post(
        "/v2/console/iap/apple/validate",
        Some(&auth),
        r#"{"receipt":"abc","user_id":"not-a-uuid"}"#,
    );
    assert_eq!(reply.refusal(INVALID_ARGUMENT), "Invalid account id.");

    // …and a well-formed one reaches the database that is not there.
    let reply = server.post(
        "/v2/console/iap/apple/validate",
        Some(&auth),
        &format!(r#"{{"receipt":"abc","user_id":"{}"}}"#, Uuid::new_v4()),
    );
    assert_eq!(reply.refusal(UNAVAILABLE), NO_DB);
}

#[test]
fn reset_and_product_delete_answer_for_their_absent_rows_and_database() {
    let server = V2Server::start(v2_state());
    let admin = token_for(ConsoleRole::Admin);

    let reply = server.delete("/v2/console/iap/huawei/reset", Some(&admin));
    assert_eq!(reply.refusal(UNAVAILABLE), NO_DB);

    let reply = server.delete("/v2/console/subscription/season-pass", Some(&admin));
    assert_eq!(reply.refusal(UNAVAILABLE), NO_DB);

    // An empty product id cannot survive routing, but a whitespace one reaches
    // the handler and is refused by name before the database is consulted.
    let reply = server.delete("/v2/console/subscription/%20", Some(&admin));
    assert_eq!(reply.refusal(INVALID_ARGUMENT), "A product id is required.");
}
