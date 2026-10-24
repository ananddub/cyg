//! `/v2/storage/index*` over the socket plane with no database behind it: the
//! credential and the definition rules are answered first, and a request that
//! passes both is stopped by the absence of the schema.

use crate::error::{INVALID_ARGUMENT, UNAUTHENTICATED, UNAVAILABLE};

use super::server::V2Server;
use super::{player_token, v2_state};

/// The smallest body that defines an index: a name, a collection, one value key.
const VALID: &str = r#"{"name":"rankings","collection":"arena","fields":["score"]}"#;

const NO_DB: &str = "The database is not configured, set [control.session] database_url.";

#[test]
fn every_index_route_demands_a_player_token() {
    let server = V2Server::start(v2_state());

    for (method, path, body) in [
        ("POST", "/v2/storage/index".to_owned(), VALID.to_owned()),
        ("GET", "/v2/storage/index".to_owned(), String::new()),
        (
            "DELETE",
            "/v2/storage/index/rankings".to_owned(),
            String::new(),
        ),
        (
            "POST",
            "/v2/storage/index/rankings/query".to_owned(),
            r#"{"query":"score:10"}"#.to_owned(),
        ),
    ] {
        let reply = server.send(&method, &path, None, &body);
        assert_eq!(
            reply.refusal(UNAUTHENTICATED),
            "Auth token required",
            "{method} {path}"
        );

        // The envelope is the uniform one: code, message and a details list.
        let value = reply.json();
        assert_eq!(value["error"]["code"].as_u64(), Some(16));
        assert!(value["error"]["details"].is_array());
    }
}

#[test]
fn a_definition_that_breaks_the_rules_is_refused_before_the_database() {
    let server = V2Server::start(v2_state());
    let auth = player_token();

    let cases = [
        (
            r#"{"collection":"arena","fields":["score"]}"#,
            "Storage index name must be set.",
        ),
        (
            r#"{"name":"rankings","fields":["score"]}"#,
            "Storage index collection must be set.",
        ),
        (
            r#"{"name":"rankings","collection":"arena","fields":[]}"#,
            "Storage index fields must name at least one top-level key of the object value.",
        ),
        (
            r#"{"name":"rankings","collection":"arena","fields":["a","b","c","d","e"]}"#,
            "Storage index fields must name at most four top-level keys.",
        ),
        (
            r#"{"name":"rankings","collection":"arena","fields":["score"],"max_entries":0}"#,
            "Storage index max entries must be greater than zero.",
        ),
    ];
    for (body, message) in cases {
        let reply = server.post("/v2/storage/index", Some(&auth), body);
        assert_eq!(reply.refusal(INVALID_ARGUMENT), message, "body: {body}");
    }
}

#[test]
fn a_query_that_makes_no_sense_is_refused_before_the_database() {
    let server = V2Server::start(v2_state());
    let auth = player_token();

    let reply = server.post(
        "/v2/storage/index/rankings/query",
        Some(&auth),
        r#"{"query":"nocolon"}"#,
    );
    assert_eq!(
        reply.refusal(INVALID_ARGUMENT),
        "Failed to parse the index query."
    );

    let reply = server.post(
        "/v2/storage/index/rankings/query",
        Some(&auth),
        r#"{"query":"","limit":500}"#,
    );
    assert_eq!(
        reply.refusal(INVALID_ARGUMENT),
        "Invalid limit - limit must be between 1 and 100."
    );

    let reply = server.post(
        "/v2/storage/index/rankings/query",
        Some(&auth),
        r#"{"query":"","cursor":"not-a-cursor!!"}"#,
    );
    assert_eq!(
        reply.refusal(INVALID_ARGUMENT),
        "Malformed cursor was used."
    );
}

#[test]
fn well_formed_index_requests_reach_the_database() {
    let server = V2Server::start(v2_state());
    let auth = player_token();

    for (method, path, body) in [
        ("POST", "/v2/storage/index".to_owned(), VALID.to_owned()),
        ("GET", "/v2/storage/index".to_owned(), String::new()),
        (
            "DELETE",
            "/v2/storage/index/rankings".to_owned(),
            String::new(),
        ),
        (
            "POST",
            "/v2/storage/index/rankings/query".to_owned(),
            r#"{"query":"score:[10 TO 20}","order":["-update_time"],"limit":10}"#.to_owned(),
        ),
    ] {
        let reply = server.send(&method, &path, Some(&auth), &body);
        assert_eq!(reply.refusal(UNAVAILABLE), NO_DB, "{method} {path}");
    }
}
