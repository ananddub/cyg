//! `GET /v2/storage/{collection}/{key}/{user_id}` — the single-object read over
//! the socket plane: the credential and the owner id are answered before the
//! database, and a well-formed read is stopped by its absence.

use uuid::Uuid;

use super::server::V2Server;
use super::{player_token, v2_state};
use crate::error::{INVALID_ARGUMENT, UNAUTHENTICATED, UNAVAILABLE};

#[test]
fn a_single_read_needs_a_player_token() {
    let server = V2Server::start(v2_state());
    let path = format!("/v2/storage/save/slot1/{}", Uuid::new_v4());

    let reply = server.get(&path, None);
    assert_eq!(reply.refusal(UNAUTHENTICATED), "Auth token required");
}

#[test]
fn a_malformed_owner_is_a_bad_request_naming_the_id() {
    let server = V2Server::start(v2_state());
    let reply = server.get("/v2/storage/save/slot1/not-a-uuid", Some(&player_token()));
    assert_eq!(
        reply.refusal(INVALID_ARGUMENT),
        "Invalid user ID - make sure user ID is a valid UUID."
    );
}

#[test]
fn a_well_formed_read_reaches_the_database() {
    let server = V2Server::start(v2_state());
    let auth = player_token();
    let no_db = "The database is not configured, set [control.session] database_url.";

    // The all-zero owner means "the caller's own object", the REST spelling of it.
    let reply = server.get(
        "/v2/storage/save/slot1/00000000-0000-0000-0000-000000000000",
        Some(&auth),
    );
    assert_eq!(reply.refusal(UNAVAILABLE), no_db);

    // A version token narrows the read to that exact state of the object and is
    // otherwise just another well-formed request.
    let reply = server.get(
        &format!(
            "/v2/storage/save/slot1/{}?version=d41d8cd98f00b204e9800998ecf8427e",
            Uuid::new_v4()
        ),
        Some(&auth),
    );
    assert_eq!(reply.refusal(UNAVAILABLE), no_db);
}
