use uuid::Uuid;

use super::*;
use crate::tests::games::{db, skipped_without_database};

#[tokio::test]
async fn a_bare_user_reads_back_by_id_and_username() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let id = Uuid::new_v4();
    q.create_user_bare(id, "bare-user")
        .await
        .expect("the user registers");

    assert_eq!(q.get_user_by_id(id).await.expect("by id reads").id, id);
    assert_eq!(
        q.get_user_by_username("bare-user")
            .await
            .expect("by username reads")
            .id,
        id
    );
}

#[tokio::test]
async fn a_device_bound_account_is_created_once() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let user = new_user(Uuid::new_v4(), "device-user");
    let device = format!("device-{}", Uuid::new_v4());

    assert!(
        q.create_device_account(&device, &user)
            .await
            .expect("the first call creates")
    );
    assert!(
        !q.create_device_account(&device, &user)
            .await
            .expect("the second call finds the device")
    );
    let row = q
        .get_device_by_id(&device)
        .await
        .expect("the device reads back");
    assert_eq!(row.user_id, user.id);
}
