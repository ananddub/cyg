//! The fixtures the console tests share: the socket plane, the bootstrap admin's
//! default credentials, and a token minted for a role no row has to hold.

pub(super) use super::super::server::V2Server;
pub(super) use super::super::{player_token, v2_state};

use crate::infra::console::{self, ConsoleRole};

/// `[control.console]` defaults: the bootstrap superadmin a fresh database has.
pub(super) const ADMIN: &str = "admin";
pub(super) const PASSWORD: &str = "password";

/// The nil id: a syntactically valid account id no table has ever heard of.
pub(super) const NOBODY: &str = "00000000-0000-0000-0000-000000000000";

/// Sign in and take the token the console answered with.
pub(super) fn login(server: &V2Server, username: &str, password: &str) -> String {
    let body = format!(r#"{{"username":"{username}","password":"{password}"}}"#);
    let reply = server.post("/v2/console/authenticate", None, &body);
    assert_eq!(reply.status, 200, "login failed: {}", reply.body);
    reply.json()["token"]
        .as_str()
        .expect("a login is answered with a token")
        .to_owned()
}

/// A console token for a role the bootstrap account does not have. The role travels
/// inside the token, so the gates can be tested without a `console_user` row.
pub(super) fn token_for(role: ConsoleRole) -> String {
    let token = console::mint("", "role-probe", "role-probe@console.local", role);
    format!("Bearer {token}")
}
