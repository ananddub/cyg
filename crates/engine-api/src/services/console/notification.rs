//! `/v2/console/notification` — a player's notifications, and sending one.

use axum::Json;
use uuid::Uuid;

use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::console::notification::{NotificationQuery, NotificationRow, SendNotificationRequest};
use crate::error::V2Error;
use crate::repositories::console_mutations::InsertNotificationParams;
use crate::ApiState;

impl super::ConsoleService {
    pub async fn list_notifications(
        &self,
        state: &ApiState,
        query: NotificationQuery,
    ) -> Result<Json<Vec<NotificationRow>>, V2Error> {
        let user_id = super::account_id(&query.user_id)?;
        let rows = super::queries(state)?
            .list_notifications_for_user(user_id, query.limit())
            .await?;
        Ok(Json(rows.into_iter().map(NotificationRow::from).collect()))
    }

    /// Send a player a notification. The sender is the signed-in operator, taken
    /// from the token rather than the body: a message must not be able to claim a
    /// sender it did not have.
    pub async fn send_notification(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        body: SendNotificationRequest,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        let user_id = super::account_id(&body.user_id)?;
        let subject = body.subject.trim();
        if subject.is_empty() {
            return Err(V2Error::invalid_argument("A notification needs a subject."));
        }

        let queries = super::queries(state)?;
        // Read first, so sending to an unknown account is a 404 rather than a row
        // that is written and never readable.
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

    /// Delete a notification by id, as the console removes it outright.
    pub async fn delete_notification(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_write()?;
        let id = super::id(id, "notification")?;
        let removed = crate::services::console::statements::delete_notification(
            super::pool(state)?,
            id,
        )
        .await
        .map_err(V2Error::from_sqlx)?;
        if !removed {
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
