pub(super) use super::super::server::V2Server;
pub(super) use super::super::{player_token, v2_state};

use crate::infra::console::{self, ConsoleRole};

pub(super) const ADMIN: &str = "admin";
pub(super) const PASSWORD: &str = "password";

pub(super) const NOBODY: &str = "00000000-0000-0000-0000-000000000000";

pub(super) fn login(server: &V2Server, username: &str, password: &str) -> String {
    let body = format!(r#"{{"username":"{username}","password":"{password}"}}"#);
    let reply = server.post("/v2/console/authenticate", None, &body);
    assert_eq!(reply.status, 200, "login failed: {}", reply.body);
    reply.json()["token"]
        .as_str()
        .expect("a login is answered with a token")
        .to_owned()
}

pub(super) fn token_for(role: ConsoleRole) -> String {
    let token = console::mint("", "role-probe", "role-probe@console.local", role);
    format!("Bearer {token}")
}
