use uuid::Uuid;

use crate::error::{FAILED_PRECONDITION, PERMISSION_DENIED, UNAUTHENTICATED};
use crate::infra::console::{self, ConsoleRole};

use super::helpers::{ADMIN, NOBODY, PASSWORD, V2Server, login, player_token, token_for, v2_state};

#[test]
fn a_write_needs_a_console_token() {
    let server = V2Server::start(v2_state());
    let path = format!("/v2/console/account/{NOBODY}/ban");

    let reply = server.post(&path, None, r#"{"reason":"test"}"#);
    assert_eq!(reply.refusal(UNAUTHENTICATED), "Console token required");

    let reply = server.post(&path, Some(&player_token()), r#"{"reason":"test"}"#);
    assert_eq!(reply.refusal(UNAUTHENTICATED), "Console token invalid");

    let reply = server.delete("/v2/console/mfa", None);
    assert_eq!(reply.refusal(UNAUTHENTICATED), "Console token required");
}

#[test]
fn a_read_only_console_user_cannot_write() {
    let server = V2Server::start(v2_state());
    let auth = token_for(ConsoleRole::Readonly);

    let reply = server.post(
        &format!("/v2/console/account/{NOBODY}/ban"),
        Some(&auth),
        r#"{"reason":"test"}"#,
    );
    assert_eq!(
        reply.refusal(PERMISSION_DENIED),
        "This console user has read-only access."
    );

    let reply = server.get("/v2/console/account", Some(&auth));
    assert_eq!(reply.status, 503, "{}", reply.body);
}

#[test]
fn only_an_admin_manages_operators_settings_and_templates() {
    let server = V2Server::start(v2_state());
    let auth = token_for(ConsoleRole::Maintainer);
    let user = format!("/v2/console/user/{NOBODY}");

    for (method, path, body) in [
        ("POST", "/v2/console/user".to_owned(), "{}"),
        ("PUT", user.clone(), "{}"),
        ("DELETE", user.clone(), ""),
        ("POST", format!("{user}/password"), "{}"),
        ("POST", format!("{user}/disable"), ""),
        ("POST", format!("{user}/enable"), ""),
        ("DELETE", format!("{user}/mfa"), ""),
        ("PUT", "/v2/console/setting".to_owned(), "{}"),
        ("DELETE", "/v2/console/setting/example".to_owned(), ""),
        ("POST", "/v2/console/acl/template".to_owned(), "{}"),
        ("PUT", format!("/v2/console/acl/template/{NOBODY}"), "{}"),
        ("DELETE", format!("/v2/console/acl/template/{NOBODY}"), ""),
        ("DELETE", format!("/v2/console/account/{NOBODY}"), ""),
        ("POST", "/v2/console/account/import".to_owned(), "null"),
        (
            "DELETE",
            "/v2/console/storage/collection/example".to_owned(),
            "",
        ),
    ] {
        let reply = server.send(method, &path, Some(&auth), body);
        assert_eq!(
            reply.refusal(PERMISSION_DENIED),
            "This console action requires the admin role.",
            "{method} {path}"
        );
    }
}

#[test]
fn a_write_role_passes_the_gate_and_reaches_the_database() {
    let server = V2Server::start(v2_state());
    let auth = token_for(ConsoleRole::Maintainer);
    let id = Uuid::new_v4();

    for (method, path, body) in [
        (
            "POST",
            format!("/v2/console/account/{id}/ban"),
            r#"{"reason":"test"}"#.to_owned(),
        ),
        (
            "POST",
            format!("/v2/console/account/{id}/unban"),
            String::new(),
        ),
        (
            "POST",
            format!("/v2/console/account/{id}/note"),
            r#"{"note":"a note"}"#.to_owned(),
        ),
        (
            "POST",
            format!("/v2/console/account/{id}/wallet"),
            r#"{"changeset":{"gems":5}}"#.to_owned(),
        ),
        (
            "DELETE",
            format!("/v2/console/account/{id}/note/{id}"),
            String::new(),
        ),
        (
            "GET",
            format!("/v2/console/account/{id}/export"),
            String::new(),
        ),
        ("DELETE", format!("/v2/console/group/{id}"), String::new()),
        (
            "DELETE",
            format!("/v2/console/storage/object/c/k/{id}"),
            String::new(),
        ),
        (
            "POST",
            "/v2/console/notification".to_owned(),
            format!(r#"{{"user_id":"{id}","subject":"test"}}"#),
        ),
    ] {
        let reply = server.send(method, &path, Some(&auth), &body);
        assert_eq!(reply.status, 503, "{method} {path}: {}", reply.body);
    }
}

#[test]
fn an_admin_write_passes_the_gate_too() {
    let server = V2Server::start(v2_state());
    let auth = format!("Bearer {}", login(&server, ADMIN, PASSWORD));
    let id = Uuid::new_v4();

    let reply = server.delete(&format!("/v2/console/user/{id}"), Some(&auth));
    assert_eq!(reply.status, 503, "{}", reply.body);
    let reply = server.put(
        "/v2/console/setting",
        Some(&auth),
        r#"{"name":"example","value":1}"#,
    );
    assert_eq!(reply.status, 503, "{}", reply.body);
}

#[test]
fn a_console_user_cannot_delete_or_disable_itself() {
    let server = V2Server::start(v2_state());
    let id = Uuid::new_v4();
    let token = console::mint(&id.to_string(), "self", "", ConsoleRole::Admin);
    let auth = format!("Bearer {token}");

    let reply = server.delete(&format!("/v2/console/user/{id}"), Some(&auth));
    assert_eq!(
        reply.refusal(FAILED_PRECONDITION),
        "A console user cannot delete itself."
    );

    let reply = server.post(&format!("/v2/console/user/{id}/disable"), Some(&auth), "");
    assert_eq!(
        reply.refusal(FAILED_PRECONDITION),
        "A console user cannot disable itself."
    );
}
