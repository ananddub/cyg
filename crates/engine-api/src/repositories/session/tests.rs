use uuid::Uuid;

use super::*;
use crate::tests::games::{db, skipped_without_database};

#[tokio::test]
async fn a_tombstone_stores_and_is_removed() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let user = Uuid::new_v4();
    assert!(q.get_user_tombstone(user).await.is_err(), "nothing yet");

    q.create_tombstone(user).await.expect("the tombstone stores");
    let row = q.get_user_tombstone(user).await.expect("the tombstone reads");
    assert_eq!(row.user_id, user);

    q.delete_user_tombstone(user)
        .await
        .expect("the tombstone clears");
    assert!(q.get_user_tombstone(user).await.is_err());
}
