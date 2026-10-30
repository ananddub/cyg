use axum::Json;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::console::notification::{
    NotificationQuery, NotificationRow, SendNotificationRequest,
};
use crate::error::V2Error;
use crate::services::deps::q;
use crate::repositories::console_mutations::InsertNotificationParams;

impl super::ConsoleService {
    pub async fn list_notifications(
        &self,
        state: &ApiState,
        query: NotificationQuery,
    ) -> Result<Json<Vec<NotificationRow>>, V2Error> {
        let user_id = super::account_id(&query.user_id)?;
        crate::utils::guards::require_account(state, query.game_id, user_id).await?;
        let rows = q(state).await?
            .list_notifications_for_user(user_id, query.limit())
            .await?;
        Ok(Json(rows.into_iter().map(NotificationRow::from).collect()))
    }

    pub async fn send_notification(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        game: Option<Uuid>,
        body: SendNotificationRequest,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        let user_id = super::account_id(&body.user_id)?;
        crate::utils::guards::require_account(state, game, user_id).await?;
        let subject = body.subject.trim();
        if subject.is_empty() {
            return Err(V2Error::invalid_argument("A notification needs a subject."));
        }

        let queries = q(state).await?;
        queries.get_user_by_id(user_id).await?;
        queries
            .insert_notification(&InsertNotificationParams {
                id: Uuid::new_v4(),
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
            state,
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
        state: &ApiState,
        claims: &ConsoleAuthed,
        game: Option<Uuid>,
        id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        let id = super::id(id, "notification")?;
        if !crate::services::games::tenant::delete_notification(state, game, id).await? {
            return Err(V2Error::not_found("Notification not found."));
        }
        super::audit(
            state,
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
