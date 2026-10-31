use std::collections::HashMap;

use uuid::Uuid;

use super::helpers::{V2Server, player_token, server_key, v2_state};
use crate::error::{INVALID_ARGUMENT, UNAUTHENTICATED, UNAVAILABLE};
use crate::infra::jwt;

#[test]
fn refresh_is_a_server_key_route_and_logout_a_player_one() {
    let server = V2Server::start(v2_state());
    let key = server_key();

    let anonymous = server.post("/v2/account/session/refresh", None, "{}");
    assert_eq!(anonymous.refusal(UNAUTHENTICATED), "Server key required");
    let nameless = server.post("/v2/account/session/refresh", Some(&key), "{}");
    assert_eq!(
        nameless.refusal(INVALID_ARGUMENT),
        "Refresh token is required."
    );

    let (token, refresh) = jwt::mint_token_pair(Uuid::new_v4(), "player", HashMap::new());
    let wrong_kind = server.post(
        "/v2/account/session/refresh",
        Some(&key),
        &format!(r#"{{"token":"{token}"}}"#),
    );
    assert_eq!(
        wrong_kind.refusal(UNAUTHENTICATED),
        "Refresh token invalid or expired."
    );

    let genuine = server.post(
        "/v2/account/session/refresh",
        Some(&key),
        &format!(r#"{{"token":"{refresh}"}}"#),
    );
    assert_eq!(
        genuine.refusal(UNAVAILABLE),
        "The database is not configured, set [control.session] database_endpoint."
    );

    let bearer = player_token();
    let logged_out = server.post("/v2/session/logout", Some(&bearer), "{}");
    assert_eq!(logged_out.status, 200);
    assert_eq!(logged_out.json(), serde_json::json!({}));

    let anonymous_logout = server.post("/v2/session/logout", None, "{}");
    assert_eq!(
        anonymous_logout.refusal(UNAUTHENTICATED),
        "Auth token required"
    );
}

#[test]
fn a_logout_can_only_revoke_the_tokens_of_the_session_that_asks() {
    let server = V2Server::start(v2_state());
    let (mine, _) = jwt::mint_token_pair(Uuid::new_v4(), "mine", HashMap::new());
    let (someone_elses, _) = jwt::mint_token_pair(Uuid::new_v4(), "other", HashMap::new());

    let reply = server.post(
        "/v2/session/logout",
        Some(&format!("Bearer {mine}")),
        &format!(r#"{{"token":"{someone_elses}"}}"#),
    );
    assert_eq!(reply.refusal(INVALID_ARGUMENT), "Session token invalid.");
}
