use uuid::Uuid;

use crate::error::{INVALID_ARGUMENT, PERMISSION_DENIED, UNAUTHENTICATED};
use crate::infra::console::ConsoleRole;

use super::helpers::{V2Server, token_for, v2_state};

const VALID: &str = r#"{"id":"weekly-arena","title":"Weekly","duration":3600}"#;

#[test]
fn a_tournament_write_needs_a_console_token() {
    let server = V2Server::start(v2_state());

    let reply = server.post("/v2/console/tournament", None, VALID);
    assert_eq!(reply.refusal(UNAUTHENTICATED), "Console token required");

    let body = reply.json();
    assert_eq!(body["error"]["code"].as_u64(), Some(16));
    assert!(body["error"]["details"].is_array());
}

#[test]
fn a_read_only_operator_cannot_write_boards_or_delete_rows() {
    let server = V2Server::start(v2_state());
    let auth = token_for(ConsoleRole::Readonly);
    let id = Uuid::new_v4();

    for (method, path, body) in [
        (
            "POST",
            "/v2/console/tournament".to_owned(),
            VALID.to_owned(),
        ),
        (
            "PUT",
            "/v2/console/tournament/weekly-arena".to_owned(),
            VALID.to_owned(),
        ),
        (
            "DELETE",
            "/v2/console/tournament/weekly-arena".to_owned(),
            String::new(),
        ),
        (
            "DELETE",
            format!("/v2/console/leaderboard/{id}"),
            String::new(),
        ),
        (
            "DELETE",
            format!("/v2/console/notification/{id}"),
            String::new(),
        ),
    ] {
        let reply = server.send(method, &path, Some(&auth), &body);
        assert_eq!(
            reply.refusal(PERMISSION_DENIED),
            "This console user has read-only access.",
            "{method} {path}"
        );
    }
}

#[test]
fn a_bad_tournament_definition_is_refused_before_the_database() {
    let server = V2Server::start(v2_state());
    let auth = token_for(ConsoleRole::Maintainer);

    let reply = server.post("/v2/console/tournament", Some(&auth), "{}");
    assert_eq!(
        reply.refusal(INVALID_ARGUMENT),
        "Tournament ID must be provided."
    );

    let reply = server.post(
        "/v2/console/tournament",
        Some(&auth),
        r#"{"id":"weekly-arena"}"#,
    );
    assert_eq!(
        reply.refusal(INVALID_ARGUMENT),
        "A tournament needs a duration of at least one second."
    );

    let reply = server.post(
        "/v2/console/tournament",
        Some(&auth),
        r#"{"id":"weekly-arena","duration":60,"reset_schedule":"not a cron"}"#,
    );
    assert_eq!(
        reply.refusal(INVALID_ARGUMENT),
        "leaderboard reset schedule is not a cron expression"
    );
}

#[test]
fn a_write_role_passes_the_board_gates_and_reaches_the_database() {
    let server = V2Server::start(v2_state());
    let auth = token_for(ConsoleRole::Maintainer);
    let id = Uuid::new_v4();
    let no_db = "The database is not configured, set [control.session] database_endpoint.";

    for (method, path, body) in [
        (
            "POST",
            "/v2/console/tournament".to_owned(),
            VALID.to_owned(),
        ),
        (
            "PUT",
            "/v2/console/tournament/weekly-arena".to_owned(),
            VALID.to_owned(),
        ),
        (
            "DELETE",
            "/v2/console/tournament/weekly-arena".to_owned(),
            String::new(),
        ),
        (
            "DELETE",
            "/v2/console/leaderboard/weekly-arena".to_owned(),
            String::new(),
        ),
        (
            "DELETE",
            format!("/v2/console/notification/{id}"),
            String::new(),
        ),
    ] {
        let reply = server.send(method, &path, Some(&auth), &body);
        assert_eq!(
            reply.refusal(crate::error::UNAVAILABLE),
            no_db,
            "{method} {path}"
        );
    }
}
