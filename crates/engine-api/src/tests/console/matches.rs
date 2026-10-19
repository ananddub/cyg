//! The match REST surface over a live socket: the player routes are session-gated,
//! the console routes are console-token gated, and every one of them reads the
//! process-wide registry rather than a database, so they answer with no pool behind
//! the plane.

use uuid::Uuid;

use crate::error::{INVALID_ARGUMENT, NOT_FOUND, PERMISSION_DENIED, UNAUTHENTICATED};
use crate::infra::console::ConsoleRole;
use crate::services::realtime::RealtimeMatchService;

use super::helpers::{ADMIN, PASSWORD, V2Server, login, player_token, token_for, v2_state};

#[test]
fn the_player_match_routes_gate_on_a_session() {
    let server = V2Server::start(v2_state());
    for path in ["/v2/match", "/v2/matchmaker/stats"] {
        // With no credential at all the gate refuses and names what is missing.
        let reply = server.get(path, None);
        assert_eq!(reply.refusal(UNAUTHENTICATED), "Auth token required", "{path}");
        // A malformed bearer is an invalid session, not a missing one.
        let reply = server.get(path, Some("Bearer not.a.jwt"));
        assert_eq!(reply.refusal(UNAUTHENTICATED), "Auth token invalid", "{path}");
    }
}

#[test]
fn a_live_session_reaches_the_match_read_surface() {
    let server = V2Server::start(v2_state());
    let auth = player_token();

    // The session list wraps its rows and prints `match_id`/`size`; the shape is the
    // contract, so the count is left to whatever else is running in the process.
    let reply = server.get("/v2/match", Some(&auth));
    assert_eq!(reply.status, 200, "{}", reply.body);
    let body = reply.json();
    if let Some(matches) = body.get("matches") {
        for row in matches.as_array().unwrap() {
            assert!(row["match_id"].is_string());
            assert_eq!(row["authoritative"], false);
            assert!(row["size"].is_number());
        }
    }

    let reply = server.get("/v2/matchmaker/stats", Some(&auth));
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert!(reply.json()["ticket_count"].is_number());
}

#[test]
fn the_console_match_routes_demand_a_console_token() {
    let server = V2Server::start(v2_state());
    let reply = server.get("/v2/console/match", None);
    assert_eq!(reply.refusal(UNAUTHENTICATED), "Console token required");
    let reply = server.get("/v2/console/match", Some(&player_token()));
    assert_eq!(reply.refusal(UNAUTHENTICATED), "Console token invalid");
}

#[test]
fn the_console_lists_running_matches_without_a_database() {
    let server = V2Server::start(v2_state());
    let auth = format!("Bearer {}", login(&server, ADMIN, PASSWORD));
    let id = RealtimeMatchService::create_match_with_tokens(15, vec!["t".to_owned()]);

    let reply = server.get("/v2/console/match", Some(&auth));
    assert_eq!(reply.status, 200, "{}", reply.body);
    let matches = reply.json()["matches"]
        .as_array()
        .expect("the console list wraps its rows")
        .clone();
    assert!(
        matches
            .iter()
            .any(|row| row["api_match"]["match_id"] == id.simple().to_string()),
        "the running match must be listed: {}",
        reply.body
    );
    // Every row carries the node field, empty on this single-process plane.
    assert!(matches.iter().all(|row| row["node"].is_string()));

    RealtimeMatchService::terminate_match(id);
}

#[test]
fn reading_a_match_state_names_the_missing_match() {
    let server = V2Server::start(v2_state());
    let auth = format!("Bearer {}", login(&server, ADMIN, PASSWORD));
    let reply = server.get(&format!("/v2/console/match/{}/state", Uuid::new_v4()), Some(&auth));
    assert_eq!(
        reply.refusal(INVALID_ARGUMENT),
        "Match not found, or match handler already stopped."
    );
}

#[test]
fn a_running_match_reports_its_state() {
    let server = V2Server::start(v2_state());
    let auth = format!("Bearer {}", login(&server, ADMIN, PASSWORD));
    let id = RealtimeMatchService::create_match_with_tokens(5, vec!["t".to_owned()]);

    let reply = server.get(&format!("/v2/console/match/{id}/state"), Some(&auth));
    assert_eq!(reply.status, 200, "{}", reply.body);
    let body = reply.json();
    assert!(body["presences"].is_array());
    assert_eq!(body["tick"], 0);
    assert_eq!(body["state"], "");

    RealtimeMatchService::terminate_match(id);
}

#[test]
fn terminating_a_match_is_a_write_the_readonly_cannot_make() {
    let server = V2Server::start(v2_state());
    let id = Uuid::new_v4();

    let read_only = token_for(ConsoleRole::Readonly);
    let reply = server.delete(&format!("/v2/console/match/{id}"), Some(&read_only));
    assert_eq!(
        reply.refusal(PERMISSION_DENIED),
        "This console user has read-only access."
    );

    let auth = format!("Bearer {}", login(&server, ADMIN, PASSWORD));
    let reply = server.delete(&format!("/v2/console/match/{id}"), Some(&auth));
    assert_eq!(reply.refusal(NOT_FOUND), "Match not found.");

    // A match that is really running goes on the admin's say-so.
    let live = RealtimeMatchService::create_match_with_tokens(0, vec![]);
    let reply = server.delete(&format!("/v2/console/match/{live}"), Some(&auth));
    assert_eq!(reply.status, 200, "{}", reply.body);
}

#[test]
fn the_route_explorer_lists_the_match_surface() {
    let server = V2Server::start(v2_state());
    let auth = format!("Bearer {}", login(&server, ADMIN, PASSWORD));
    let reply = server.get("/v2/console/route", Some(&auth));
    assert_eq!(reply.status, 200, "{}", reply.body);
    let routes = reply.json();
    let routes = routes.as_array().expect("the explorer lists routes");
    let has = |method: &str, path: &str| {
        routes
            .iter()
            .any(|r| r["method"] == method && r["path"] == path)
    };
    assert!(has("GET", "/v2/match"), "{}", reply.body);
    assert!(has("GET", "/v2/matchmaker/stats"), "{}", reply.body);
    assert!(has("GET", "/v2/console/match"), "{}", reply.body);
    assert!(
        has("GET", "/v2/console/match/{id}/state") || has("GET", "/v2/console/match/:id/state"),
        "{}",
        reply.body
    );
    assert!(
        has("DELETE", "/v2/console/match/{id}") || has("DELETE", "/v2/console/match/:id"),
        "{}",
        reply.body
    );
}
