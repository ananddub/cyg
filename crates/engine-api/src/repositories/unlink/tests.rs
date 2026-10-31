use uuid::Uuid;

use super::*;
use crate::repositories::auth::CreateUserWithEmailParams;
use crate::tests::games::{db, skipped_without_database};

#[tokio::test]
async fn unlinking_the_last_identity_is_refused_until_a_device_exists() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let id = Uuid::new_v4();
    q.create_user_with_email(&CreateUserWithEmailParams {
        id,
        username: "unlink-user".into(),
        email: "unlink@example.com".into(),
        password: b"hashed".to_vec(),
    })
    .await
    .expect("the user registers");

    assert!(
        q.unlink_email(id).await.is_err(),
        "the only identity cannot be dropped"
    );

    q.insert_device(&format!("device-{}", Uuid::new_v4()), id)
        .await
        .expect("the device registers");
    assert_eq!(q.unlink_email(id).await.expect("the email unlinks"), id);
    assert!(q.get_user_by_email("unlink@example.com").await.is_err());
}
