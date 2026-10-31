use uuid::Uuid;

use super::*;
use crate::tests::games::{db, skipped_without_database};

#[tokio::test]
async fn an_email_link_resolves_by_email() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let id = Uuid::new_v4();
    q.create_user_bare(id, "link-user")
        .await
        .expect("the user registers");

    q.link_email(&LinkEmailParams {
        email: "link@example.com".into(),
        d: b"hashed".to_vec(),
        id,
    })
    .await
    .expect("the email links");
    let row = q
        .get_user_by_email("link@example.com")
        .await
        .expect("the email resolves");
    assert_eq!(row.id, id);
    assert_eq!(row.password.as_deref(), Some(&b"hashed"[..]));
}
