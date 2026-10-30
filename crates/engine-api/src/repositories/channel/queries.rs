use super::*;

impl Queries {
    pub async fn insert_channel_message(
        &self,
        params: &InsertChannelMessageParams,
    ) -> Result<Message, DbError> {
        let now = Utc::now();
        let message = Message {
            id: params.id,
            code: 0,
            sender_id: params.sender_id,
            username: params.username.clone(),
            stream_mode: params.stream_mode,
            stream_subject: params.stream_subject,
            stream_descriptor: params.stream_descriptor,
            stream_label: params.stream_label.clone(),
            content: params.content.clone(),
            create_time: now,
            update_time: now,
        };
        self.create("message", Key::Uuid(params.id), &message).await
    }
    pub async fn list_channel_messages(
        &self,
        params: &ListChannelMessagesParams,
    ) -> Result<Vec<Message>, DbError> {
        self.all(
            "SELECT * FROM message \
             WHERE stream_mode = $stream_mode \
               AND stream_subject = $stream_subject \
               AND stream_descriptor = $stream_descriptor \
               AND stream_label = $stream_label \
               AND create_time < $create_time \
             ORDER BY create_time DESC LIMIT $limit",
            params!(
                stream_mode = params.stream_mode,
                stream_subject = params.stream_subject.to_string(),
                stream_descriptor = params.stream_descriptor.to_string(),
                stream_label = params.stream_label,
                create_time = time::encode(params.create_time),
                limit = params.limit,
            ),
        )
        .await
    }
    pub async fn list_channel_messages_by_sender(
        &self,
        params: &ListChannelMessagesBySenderParams,
    ) -> Result<Vec<Message>, DbError> {
        self.all(
            "SELECT * FROM message \
             WHERE stream_mode = $stream_mode \
               AND stream_subject = $stream_subject \
               AND stream_descriptor = $stream_descriptor \
               AND stream_label = $stream_label \
               AND sender_id = $sender_id \
               AND create_time < $create_time \
             ORDER BY create_time DESC LIMIT $limit",
            params!(
                stream_mode = params.stream_mode,
                stream_subject = params.stream_subject.to_string(),
                stream_descriptor = params.stream_descriptor.to_string(),
                stream_label = params.stream_label,
                sender_id = params.sender_id.to_string(),
                create_time = time::encode(params.create_time),
                limit = params.limit,
            ),
        )
        .await
    }
    pub async fn get_message_by_id(&self, id: Uuid) -> Result<Message, DbError> {
        self.one(
            "SELECT * FROM type::record('message', <uuid>$id) LIMIT 1",
            params!(id = id.to_string()),
        )
        .await
    }
    pub async fn update_channel_message(
        &self,
        params: &UpdateChannelMessageParams,
    ) -> Result<Message, DbError> {
        self.one(
            "UPDATE type::record('message', <uuid>$id) \
             SET content = $content, update_time = $now \
             WHERE sender_id = $sender_id RETURN AFTER",
            params!(
                content = params.content,
                now = time::encode(Utc::now()),
                id = params.id.to_string(),
                sender_id = params.sender_id.to_string(),
            ),
        )
        .await
    }
    pub async fn delete_channel_message(
        &self,
        id: Uuid,
        sender_id: Uuid,
    ) -> Result<Message, DbError> {
        self.one(
            "UPDATE type::record('message', <uuid>$id) \
             SET code = 2, content = $content, update_time = $now \
             WHERE sender_id = $sender_id RETURN AFTER",
            params!(
                content = serde_json::json!({}),
                now = time::encode(Utc::now()),
                id = id.to_string(),
                sender_id = sender_id.to_string(),
            ),
        )
        .await
    }
    pub async fn list_channel_messages_forward(
        &self,
        params: &ListChannelMessagesForwardParams,
    ) -> Result<Vec<Message>, DbError> {
        self.all(
            "SELECT * FROM message \
             WHERE stream_mode = $stream_mode \
               AND stream_subject = $stream_subject \
               AND stream_descriptor = $stream_descriptor \
               AND stream_label = $stream_label \
               AND (create_time > $create_time \
                    OR (create_time = $create_time \
                        AND id > type::record('message', <uuid>$id))) \
             ORDER BY create_time ASC, id ASC LIMIT $limit",
            params!(
                stream_mode = params.stream_mode,
                stream_subject = params.stream_subject.to_string(),
                stream_descriptor = params.stream_descriptor.to_string(),
                stream_label = params.stream_label,
                create_time = time::encode(params.create_time),
                id = params.id.to_string(),
                limit = params.limit,
            ),
        )
        .await
    }
}

impl Queries {
    pub async fn list_channel_messages_backward(
        &self,
        params: &ListChannelMessagesBackwardParams,
    ) -> Result<Vec<Message>, DbError> {
        self.all(
            "SELECT * FROM message \
             WHERE stream_mode = $stream_mode \
               AND stream_subject = $stream_subject \
               AND stream_descriptor = $stream_descriptor \
               AND stream_label = $stream_label \
               AND (create_time < $create_time \
                    OR (create_time = $create_time \
                        AND id < type::record('message', <uuid>$id))) \
             ORDER BY create_time DESC, id DESC LIMIT $limit",
            params!(
                stream_mode = params.stream_mode,
                stream_subject = params.stream_subject.to_string(),
                stream_descriptor = params.stream_descriptor.to_string(),
                stream_label = params.stream_label,
                create_time = time::encode(params.create_time),
                id = params.id.to_string(),
                limit = params.limit,
            ),
        )
        .await
    }
}
