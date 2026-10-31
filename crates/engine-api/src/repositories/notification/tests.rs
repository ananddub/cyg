use chrono::{Duration, Utc};
use serde_json::json;
use uuid::Uuid;

use super::*;
use crate::tests::games::{db, skipped_without_database};

#[tokio::test]
async fn notifications_list_and_delete_by_id() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let user = Uuid::new_v4();
    let sender = Uuid::new_v4();
    let id = Uuid::new_v4();
    q.create_notification(&CreateNotificationParams {
        id,
        user_id: user,
        subject: "Welcome".into(),
        content: json!({"body": "hi"}),
        sender_id: sender,
        code: 1,
    })
    .await
    .expect("the notification stores");

    let listed = q
        .list_notifications(&ListNotificationsParams {
            user_id: user,
            create_time: Utc::now() + Duration::seconds(1),
            limit: 10,
        })
        .await
        .expect("the inbox lists");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].subject, "Welcome");

    assert!(
        q.delete_notification_by_id(id)
            .await
            .expect("the delete runs")
    );
    assert!(
        !q.delete_notification_by_id(id)
            .await
            .expect("the second delete is a no-op")
    );
}
