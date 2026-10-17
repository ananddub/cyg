//! The console and account routes added to close the REST gaps: each needs its
//! credential, its role gate refuses the wrong one, and a request that passes both
//! reaches the database — 503 with no DSN, which is the sound of the gate letting it
//! through. The refusals that need no rows at all (id parsing, the currency guard, the
//! bootstrap account) are asserted by their message.

use uuid::Uuid;

use crate::error::{FAILED_PRECONDITION, INVALID_ARGUMENT, PERMISSION_DENIED, UNAUTHENTICATED};
use crate::infra::console::ConsoleRole;

use super::helpers::{NOBODY, V2Server, player_token, token_for, v2_state};

#[test]
fn each_new_route_demands_a_console_token() {
    let server = V2Server::start(v2_state());
    let id = Uuid::new_v4();
    for (method, path, body) in [
        ("POST", format!("/v2/console/user/{id}/mfa/require"), String::new()),
        ("DELETE", format!("/v2/console/user/{id}/mfa/require"), String::new()),
        ("DELETE", format!("/v2/console/user/{id}/mfa/reset"), String::new()),
        ("POST", "/v2/console/password_change".to_owned(), "{}".to_owned()),
        ("GET", "/v2/console/audit/log/users".to_owned(), String::new()),
        ("POST", format!("/v2/console/account/{id}/import"), "null".to_owned()),
        ("GET", format!("/v2/console/group/{id}/export"), String::new()),
        (
            "DELETE",
            format!("/v2/console/leaderboard/board-1/owner/{id}"),
            String::new(),
        ),
    ] {
        let reply = server.send(&method, &path, None, &body);
        assert_eq!(
            reply.refusal(UNAUTHENTICATED),
            "Console token required",
            "{method} {path}"
        );
    }
}

#[test]
fn a_player_cannot_reach_a_console_route() {
    let server = V2Server::start(v2_state());
    let reply = server.get("/v2/console/audit/log/users", Some(&player_token()));
    assert_eq!(reply.refusal(UNAUTHENTICATED), "Console token invalid");
}

#[test]
fn the_mfa_flag_routes_are_admin_only_and_then_reach_the_database() {
    let server = V2Server::start(v2_state());
    let maintainer = token_for(ConsoleRole::Maintainer);
    let admin = token_for(ConsoleRole::Admin);

    // Require-on is a POST, require-off and reset are DELETEs; all three are admin-only.
    let require = format!("/v2/console/user/{NOBODY}/mfa/require");
    let reset = format!("/v2/console/user/{NOBODY}/mfa/reset");
    for (method, path) in [("POST", require.clone()), ("DELETE", require.clone()), ("DELETE", reset)] {
        let reply = server.send(method, &path, Some(&maintainer), "");
        assert_eq!(
            reply.refusal(PERMISSION_DENIED),
            "This console action requires the admin role.",
            "{method} {path}"
        );
        // Past the gate: no database is the next thing standing in the way.
        let reply = server.send(method, &path, Some(&admin), "");
        assert_eq!(reply.status, 503, "{method} {path}: {}", reply.body);
    }

    // A bad id is named before the flag is written.
    let reply = server.post("/v2/console/user/not-a-uuid/mfa/require", Some(&admin), "");
    assert_eq!(reply.refusal(INVALID_ARGUMENT), "Invalid console user id.");
}

#[test]
fn self_password_change_verifies_before_it_touches_the_database() {
    let server = V2Server::start(v2_state());
    let admin = token_for(ConsoleRole::Admin);

    // Too short a new password is refused before anything else is checked.
    let reply = server.post(
        "/v2/console/password_change",
        Some(&admin),
        r#"{"password":"short"}"#,
    );
    assert_eq!(
        reply.refusal(INVALID_ARGUMENT),
        "A console password must be at least 12 characters."
    );

    // The bootstrap token in `admin` carries no operator id, so there is no row whose
    // current password to verify — refused without a database.
    let reply = server.post(
        "/v2/console/password_change",
        Some(&admin),
        r#"{"current_password":"old","password":"a-long-enough-password"}"#,
    );
    assert_eq!(
        reply.refusal(FAILED_PRECONDITION),
        "The bootstrap console account has no password to change."
    );
}

#[test]
fn a_wallet_change_needs_a_player_token_and_currency_needs_the_console() {
    let server = V2Server::start(v2_state());

    let reply = server.send("PUT", "/v2/account/wallet", None, "{}");
    assert_eq!(reply.status, 401, "a wallet write needs a session: {}", reply.body);

    // Past the session: with no database the read of the caller's row is 503, proving
    // the `PUT` is wired to the metadata-only handler rather than left unregistered.
    let reply = server.put("/v2/account/wallet", Some(&player_token()), "{}");
    assert_eq!(reply.status, 503, "{}", reply.body);
}

#[test]
fn the_read_and_write_gates_on_group_export_audit_and_record_delete() {
    let server = V2Server::start(v2_state());
    let readonly = token_for(ConsoleRole::Readonly);
    let maintainer = token_for(ConsoleRole::Maintainer);

    // Audit users is a read: any console token passes the gate and reaches the pool.
    let reply = server.get("/v2/console/audit/log/users", Some(&readonly));
    assert_eq!(reply.status, 503, "{}", reply.body);

    // Group export is write-grade: read-only refuses, a write role reaches the query.
    let reply = server.get(&format!("/v2/console/group/{NOBODY}/export"), Some(&readonly));
    assert_eq!(
        reply.refusal(PERMISSION_DENIED),
        "This console user has read-only access."
    );
    let reply = server.get("/v2/console/group/not-a-uuid/export", Some(&maintainer));
    assert_eq!(reply.refusal(INVALID_ARGUMENT), "Invalid account id.");
    let reply = server.get(&format!("/v2/console/group/{NOBODY}/export"), Some(&maintainer));
    assert_eq!(reply.status, 503, "{}", reply.body);

    // Owner-record delete is write-grade and parses the owner before loading a board.
    let reply = server.delete(
        &format!("/v2/console/leaderboard/board-1/owner/{NOBODY}"),
        Some(&readonly),
    );
    assert_eq!(
        reply.refusal(PERMISSION_DENIED),
        "This console user has read-only access."
    );
    let reply = server.delete(
        "/v2/console/leaderboard/board-1/owner/not-a-uuid",
        Some(&maintainer),
    );
    assert_eq!(reply.refusal(INVALID_ARGUMENT), "Invalid owner ID.");
    let reply = server.delete(
        &format!("/v2/console/leaderboard/board-1/owner/{NOBODY}"),
        Some(&maintainer),
    );
    assert_eq!(reply.status, 503, "{}", reply.body);
}

#[test]
fn per_id_import_addresses_the_record_by_its_path_id() {
    let server = V2Server::start(v2_state());
    let admin = token_for(ConsoleRole::Admin);
    let reply = server.post("/v2/console/account/not-a-uuid/import", Some(&admin), "null");
    assert_eq!(reply.refusal(INVALID_ARGUMENT), "Invalid account id.");

    // A valid path id with no export document: the account the default carries (the
    // nil id) does not match the path, so it is refused before the restore runs.
    let id = Uuid::new_v4();
    let reply = server.post(&format!("/v2/console/account/{id}/import"), Some(&admin), "null");
    assert_eq!(
        reply.refusal(INVALID_ARGUMENT),
        "The account id in the body must match the id in the path."
    );
}
