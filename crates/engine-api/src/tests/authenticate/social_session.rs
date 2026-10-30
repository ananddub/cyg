use std::collections::HashMap;
use std::sync::Arc;

use engine_config::ControlConfig;

use crate::services::auth::{social, verify};
use crate::tests::games::db;

#[tokio::test]
async fn a_confirmed_provider_identity_opens_a_session_and_reuses_the_account() {
    let Some((mut state, _pool)) = db::state().await else {
        return crate::tests::games::skipped_without_database();
    };
    let mut control = ControlConfig::default();
    control.social.allow_unverified = true;
    state.control = Arc::new(control);

    let vars = HashMap::new();
    let first = social::token(
        &state,
        verify::Kind::Google,
        "e2e-google-identity-token",
        "PlayerOne",
        true,
        vars.clone(),
    )
    .await
    .expect("the verified identity opens a session")
    .0;
    assert!(first.created);
    assert!(!first.token.is_empty());

    let again = social::token(
        &state,
        verify::Kind::Google,
        "e2e-google-identity-token",
        "PlayerOne",
        false,
        vars.clone(),
    )
    .await
    .expect("the same identity returns to the same account")
    .0;
    assert!(!again.created);

    let error = social::token(
        &state,
        verify::Kind::Steam,
        "e2e-steam-identity-token",
        "PlayerTwo",
        false,
        vars,
    )
    .await
    .expect_err("an unknown identity is not created without create=true");
    assert_eq!(error.message, "User account not found.");
}

#[tokio::test]
async fn a_moderation_rejected_name_stops_before_the_provider_is_called() {
    let Some((mut state, _pool)) = db::state().await else {
        return crate::tests::games::skipped_without_database();
    };
    let mut control = ControlConfig::default();
    control.social.allow_unverified = true;
    state.control = Arc::new(control);

    let error = social::token(
        &state,
        verify::Kind::Facebook,
        "e2e-facebook-token",
        "bad[]name",
        true,
        HashMap::new(),
    )
    .await
    .expect_err("the username is refused");
    assert_eq!(
        error.message,
        "Username invalid, no spaces or control characters allowed."
    );
}
