use chrono::{Duration, Utc};
use serde_json::json;
use uuid::Uuid;

use super::*;
use crate::tests::games::{db, skipped_without_database};

#[tokio::test]
async fn a_channel_message_stores_lists_and_soft_deletes() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let sender = Uuid::new_v4();
    let subject = Uuid::new_v4();
    let descriptor = Uuid::new_v4();
    let id = Uuid::new_v4();

    q.insert_channel_message(&InsertChannelMessageParams {
        id,
        sender_id: sender,
        username: "sender".into(),
        stream_mode: 2,
        stream_subject: subject,
        stream_descriptor: descriptor,
        stream_label: "room".into(),
        content: json!({"text": "hi"}),
    })
    .await
    .expect("the message stores");

    let listed = q
        .list_channel_messages(&ListChannelMessagesParams {
            stream_mode: 2,
            stream_subject: subject,
            stream_descriptor: descriptor,
            stream_label: "room".into(),
            create_time: Utc::now() + Duration::seconds(1),
            limit: 10,
        })
        .await
        .expect("the stream lists");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].content["text"], "hi");

    let updated = q
        .update_channel_message(&UpdateChannelMessageParams {
            content: json!({"text": "edited"}),
            id,
            sender_id: sender,
        })
        .await
        .expect("the sender edits");
    assert_eq!(updated.content["text"], "edited");

    let deleted = q
        .delete_channel_message(id, sender)
        .await
        .expect("the sender deletes");
    assert_eq!(deleted.code, 2);
    let stored = q.get_message_by_id(id).await.expect("the row remains");
    assert_eq!(stored.code, 2);
}
