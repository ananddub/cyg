use super::*;

pub struct InsertChannelMessageParams {
    pub id: Uuid,
    pub sender_id: Uuid,
    pub username: String,
    pub stream_mode: i16,
    pub stream_subject: Uuid,
    pub stream_descriptor: Uuid,
    pub stream_label: String,
    pub content: serde_json::Value,
}

pub struct ListChannelMessagesParams {
    pub stream_mode: i16,
    pub stream_subject: Uuid,
    pub stream_descriptor: Uuid,
    pub stream_label: String,
    pub create_time: DateTime<Utc>,
    pub limit: i64,
}

pub struct ListChannelMessagesBySenderParams {
    pub stream_mode: i16,
    pub stream_subject: Uuid,
    pub stream_descriptor: Uuid,
    pub stream_label: String,
    pub sender_id: Uuid,
    pub create_time: DateTime<Utc>,
    pub limit: i64,
}

pub struct UpdateChannelMessageParams {
    pub content: serde_json::Value,
    pub id: Uuid,
    pub sender_id: Uuid,
}

pub struct ListChannelMessagesForwardParams {
    pub stream_mode: i16,
    pub stream_subject: Uuid,
    pub stream_descriptor: Uuid,
    pub stream_label: String,
    pub create_time: DateTime<Utc>,
    pub id: Uuid,
    pub limit: i64,
}

pub type ListChannelMessagesBackwardParams = ListChannelMessagesForwardParams;
