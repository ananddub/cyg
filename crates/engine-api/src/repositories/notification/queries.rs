use super::*;

impl Queries {
    pub async fn create_notification(
        &self,
        params: &CreateNotificationParams,
    ) -> Result<Notification, DbError> {
        let notification = Notification {
            id: params.id,
            user_id: params.user_id,
            subject: params.subject.clone(),
            content: params.content.clone(),
            code: params.code,
            sender_id: params.sender_id,
            create_time: Utc::now(),
        };
        self.create("notification", Key::Uuid(params.id), &notification)
            .await
    }
    pub async fn list_notifications(
        &self,
        params: &ListNotificationsParams,
    ) -> Result<Vec<Notification>, DbError> {
        self.all(
            "SELECT * FROM notification WHERE user_id = $user_id \
               AND create_time < $create_time \
             ORDER BY create_time DESC LIMIT $limit",
            params!(
                user_id = params.user_id.to_string(),
                create_time = time::encode(params.create_time),
                limit = params.limit,
            ),
        )
        .await
    }
    pub async fn list_notifications_by_sender(
        &self,
        params: &ListNotificationsBySenderParams,
    ) -> Result<Vec<Notification>, DbError> {
        self.all(
            "SELECT * FROM notification WHERE user_id = $user_id \
               AND sender_id = $sender_id \
               AND create_time < $create_time \
             ORDER BY create_time DESC LIMIT $limit",
            params!(
                user_id = params.user_id.to_string(),
                sender_id = params.sender_id.to_string(),
                create_time = time::encode(params.create_time),
                limit = params.limit,
            ),
        )
        .await
    }
    pub async fn list_notifications_by_code(
        &self,
        params: &ListNotificationsByCodeParams,
    ) -> Result<Vec<Notification>, DbError> {
        self.all(
            "SELECT * FROM notification WHERE user_id = $user_id \
               AND code = $code AND create_time < $create_time \
             ORDER BY create_time DESC LIMIT $limit",
            params!(
                user_id = params.user_id.to_string(),
                code = params.code,
                create_time = time::encode(params.create_time),
                limit = params.limit,
            ),
        )
        .await
    }
    pub async fn delete_notification(
        &self,
        user_id: Uuid,
        id: Uuid,
    ) -> Result<Uuid, DbError> {
        let deleted: Notification = self
            .one(
                "DELETE notification WHERE user_id = $user_id \
                 AND id = type::record('notification', <uuid>$id) RETURN BEFORE",
                params!(
                    user_id = user_id.to_string(),
                    id = id.to_string(),
                ),
            )
            .await?;
        Ok(deleted.id)
    }
    pub async fn delete_notification_by_id(
        &self,
        id: Uuid,
    ) -> Result<bool, DbError> {
        let deleted: Option<Notification> = self
            .opt(
                "DELETE type::record('notification', <uuid>$id) RETURN BEFORE",
                params!(id = id.to_string()),
            )
            .await?;
        Ok(deleted.is_some())
    }
    pub async fn delete_notifications(
        &self,
        user_id: Uuid,
    ) -> Result<(), DbError> {
        self.exec(
            "DELETE notification WHERE user_id = $user_id",
            params!(user_id = user_id.to_string()),
        )
        .await
    }
    pub async fn list_notifications_paged(
        &self,
        params: &ListNotificationsPagedParams,
    ) -> Result<Vec<Notification>, DbError> {
        self.all(
            "SELECT * FROM notification WHERE user_id = $user_id \
               AND (create_time > $create_time \
                    OR (create_time = $create_time \
                        AND id > type::record('notification', <uuid>$id_start))) \
             ORDER BY create_time ASC, id ASC LIMIT $limit",
            params!(
                user_id = params.user_id.to_string(),
                create_time = time::encode(params.create_time),
                id_start = params.id_start.to_string(),
                limit = params.limit,
            ),
        )
        .await
    }
    pub async fn list_notifications_by_sender_paged(
        &self,
        params: &ListNotificationsBySenderPagedParams,
    ) -> Result<Vec<Notification>, DbError> {
        self.all(
            "SELECT * FROM notification WHERE user_id = $user_id \
               AND sender_id = $sender_id \
               AND (create_time > $create_time \
                    OR (create_time = $create_time \
                        AND id > type::record('notification', <uuid>$id_start))) \
             ORDER BY create_time ASC, id ASC LIMIT $limit",
            params!(
                user_id = params.user_id.to_string(),
                sender_id = params.sender_id.to_string(),
                create_time = time::encode(params.create_time),
                id_start = params.id_start.to_string(),
                limit = params.limit,
            ),
        )
        .await
    }
}

impl Queries {
    pub async fn list_notifications_by_code_paged(
        &self,
        params: &ListNotificationsByCodePagedParams,
    ) -> Result<Vec<Notification>, DbError> {
        self.all(
            "SELECT * FROM notification WHERE user_id = $user_id \
               AND code = $code \
               AND (create_time > $create_time \
                    OR (create_time = $create_time \
                        AND id > type::record('notification', <uuid>$id_start))) \
             ORDER BY create_time ASC, id ASC LIMIT $limit",
            params!(
                user_id = params.user_id.to_string(),
                code = params.code,
                create_time = time::encode(params.create_time),
                id_start = params.id_start.to_string(),
                limit = params.limit,
            ),
        )
        .await
    }
}
