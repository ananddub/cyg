use axum::Json;
use uuid::Uuid;

use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::console::notification::{
    NotificationQuery, NotificationRow, SendNotificationRequest,
};
use crate::error::V2Error;
use crate::repositories::console_mutations::InsertNotificationParams;

impl super::ConsoleService {
    pub async fn list_notifications(
        &self,

        query: NotificationQuery,
    ) -> Result<Json<Vec<NotificationRow>>, V2Error> {
        let user_id = super::account_id(&query.user_id)?;
        crate::utils::guards::require_account(&self.plane, query.game_id, user_id).await?;
        let rows = self
            .plane
            .q()
            .await?
            .list_notifications_for_user(user_id, query.limit())
            .await?;
        Ok(Json(rows.into_iter().map(NotificationRow::from).collect()))
    }

    pub async fn send_notification(
        &self,

        claims: &ConsoleAuthed,
        game: Option<Uuid>,
        body: SendNotificationRequest,
        id: Uuid,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        let user_id = super::account_id(&body.user_id)?;
        crate::utils::guards::require_account(&self.plane, game, user_id).await?;
        let subject = body.subject.trim();
        if subject.is_empty() {
            return Err(V2Error::invalid_argument("A notification needs a subject."));
        }

        let queries = self.plane.q().await?;
        queries.get_user_by_id(user_id).await?;
        queries
            .insert_notification(&InsertNotificationParams {
                id,
                user_id,
                subject: subject.to_owned(),
                content: if body.content.is_null() {
                    serde_json::json!({})
                } else {
                    body.content
                },
                code: body.code,
                sender_id: claims.0.uuid().unwrap_or_else(Uuid::nil),
            })
            .await?;
        super::audit(
            &self.plane,
            claims,
            "notification.send",
            "notification",
            Some(user_id),
            &format!("Sent \"{subject}\"."),
        )
        .await;
        Ok(Json(Empty {}))
    }

    pub async fn delete_notification(
        &self,

        claims: &ConsoleAuthed,
        game: Option<Uuid>,
        id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        let id = super::id(id, "notification")?;
        if !crate::services::games::tenant::delete_notification(&self.plane, game, id).await? {
            return Err(V2Error::not_found("Notification not found."));
        }
        super::audit(
            &self.plane,
            claims,
            "notification.delete",
            "notification",
            Some(id),
            "Deleted a notification.",
        )
        .await;
        Ok(Json(Empty {}))
    }
}
