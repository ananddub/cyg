use chrono::{DateTime, Utc};
use serde_json::json;
use uuid::Uuid;

use super::*;
use crate::tests::games::{db, skipped_without_database};

#[tokio::test]
async fn console_user_mfa_flags_move_and_reset() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let id = Uuid::new_v4();
    q.insert_console_user(&InsertConsoleUserParams {
        id,
        username: "operator".into(),
        email: "operator@example.com".into(),
        acl: json!({}),
        password: b"secret".to_vec(),
        mfa_required: false,
    })
    .await
    .expect("the console user registers");

    q.set_console_user_mfa_required(id, true)
        .await
        .expect("the requirement turns on");
    let row = q
        .find_console_user_mfa("operator")
        .await
        .expect("the mfa row reads");
    assert_eq!(row.mfa_required, Some(true));

    q.reset_console_user_mfa_factor(id)
        .await
        .expect("the factor resets");
    let row = q
        .find_console_user_mfa_by_id(id)
        .await
        .expect("the mfa row reads by id");
    assert!(row.mfa_secret.is_none());

    q.clear_console_user_mfa(id)
        .await
        .expect("the factor clears");
    let row = q
        .find_console_user_mfa_by_id(id)
        .await
        .expect("the cleared row reads");
    assert_eq!(row.mfa_required, Some(false));
}

#[tokio::test]
async fn disabling_a_console_user_stamps_the_time() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let id = Uuid::new_v4();
    q.insert_console_user(&InsertConsoleUserParams {
        id,
        username: "retired".into(),
        email: "retired@example.com".into(),
        acl: json!({}),
        password: b"secret".to_vec(),
        mfa_required: false,
    })
    .await
    .expect("the console user registers");

    q.set_console_user_disabled(Utc::now(), id)
        .await
        .expect("the user disables");
    let row = q
        .find_console_user_by_id(id)
        .await
        .expect("the row reads");
    assert!(row.disable_time > DateTime::from_timestamp(0, 0).unwrap());
}
