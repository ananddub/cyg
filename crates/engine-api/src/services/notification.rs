use crate::services::plane::Plane;
use auto_di::singleton;
use axum::Json;
use std::sync::Arc;
use uuid::Uuid;

use crate::app::middleware::Authed;
use crate::db::DbError;
use crate::dto::Empty;
use crate::dto::notification::{
    DeleteRequest, ListQuery, NotificationListResponse, NotificationResponse,
};
use crate::error::V2Error;
use crate::repositories::notification::{
    ListNotificationsByCodePagedParams, ListNotificationsBySenderPagedParams,
    ListNotificationsPagedParams,
};
use crate::services::social::ts_head;

use crate::utils::notification::{
    LIMIT_RANGE, decode_cursor, encode_cursor, encode_cursor_at, flatten,
};

pub struct NotificationService {
    pub(crate) plane: Arc<Plane>,
}

#[singleton]
impl NotificationService {
    pub fn new(plane: Arc<Plane>) -> Self {
        Self { plane }
    }
}

impl NotificationService {
    pub async fn list(
        &self,

        claims: &Authed,
        req: ListQuery,
    ) -> Result<Json<NotificationListResponse>, V2Error> {
        let limit = match req.limit {
            None => 1,
            Some(n) if (1..=100).contains(&n) => n,
            Some(_) => return Err(V2Error::invalid_argument(LIMIT_RANGE)),
        };
        let cursor = decode_cursor(req.cacheable_cursor.as_deref())?;
        let (create_time, id_start) = cursor.unwrap_or((ts_head(), Uuid::nil()));
        let queries = self.plane.q().await?;
        let uid = claims.uid();

        let mut rows = match (req.sender_id.as_deref(), req.code) {
            (Some(sender), _) => {
                let sender = Uuid::parse_str(sender)
                    .map_err(|_| V2Error::invalid_argument("Invalid sender ID."))?;
                queries
                    .list_notifications_by_sender_paged(&ListNotificationsBySenderPagedParams {
                        user_id: uid,
                        sender_id: sender,
                        create_time,
                        id_start,
                        limit: limit + 1,
                    })
                    .await?
            }
            (None, Some(code)) => {
                queries
                    .list_notifications_by_code_paged(&ListNotificationsByCodePagedParams {
                        user_id: uid,
                        code,
                        create_time,
                        id_start,
                        limit: limit + 1,
                    })
                    .await?
            }
            _ => {
                queries
                    .list_notifications_paged(&ListNotificationsPagedParams {
                        user_id: uid,
                        create_time,
                        id_start,
                        limit: limit + 1,
                    })
                    .await?
            }
        };
        if rows.len() as i64 > limit {
            rows.pop();
        }

        let notifications: Vec<NotificationResponse> =
            rows.iter().map(NotificationResponse::from).collect();
        let cacheable_cursor = if let Some(last) = rows.last() {
            encode_cursor(last)
        } else {
            match req.cacheable_cursor.filter(|s| !s.is_empty()) {
                Some(raw) => raw,
                None => encode_cursor_at(ts_head(), Uuid::nil()),
            }
        };
        Ok(Json(NotificationListResponse {
            notifications,
            cacheable_cursor,
        }))
    }

    pub async fn delete_many(
        &self,

        auth: &Authed,
        req: DeleteRequest,
    ) -> Result<Json<Empty>, V2Error> {
        let uid = auth.0.uuid()?;
        let queries = self.plane.q().await?;
        for id in flatten(req.ids) {
            delete_one(&queries, uid, id).await?;
        }
        Ok(Json(Empty {}))
    }

    pub async fn delete_one(&self, auth: &Authed, id: &str) -> Result<Json<Empty>, V2Error> {
        let id = Uuid::parse_str(id)
            .map_err(|_| V2Error::invalid_argument("Invalid notification ID."))?;
        let queries = self.plane.q().await?;
        delete_one(&queries, auth.0.uuid()?, id).await?;
        Ok(Json(Empty {}))
    }
}

async fn delete_one(
    queries: &crate::repositories::db::Queries,
    uid: Uuid,
    id: Uuid,
) -> Result<(), V2Error> {
    queries
        .delete_notification(uid, id)
        .await
        .map_err(|e| match e {
            DbError::NotFound => V2Error::not_found("Notification not found."),
            other => V2Error::from(other),
        })?;
    Ok(())
}

/// What the game may ask a notification to say.
///
/// `user_id` empty means "the caller" — the common case, and the one that needs
/// no permission beyond being signed in. Naming another player is a
/// server-authoritative action and requires an operator session, so a client
/// cannot push notifications into someone else's inbox.
#[derive(poem_openapi::Object, Debug, Clone, Default, serde::Deserialize)]
pub struct PushRequest {
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub content: serde_json::Value,
    #[serde(default)]
    pub code: i16,
}

impl NotificationService {
    pub async fn send(&self, auth: &Authed, body: PushRequest) -> Result<Json<Empty>, V2Error> {
        let subject = body.subject.trim();
        if subject.is_empty() {
            return Err(V2Error::invalid_argument("A notification needs a subject."));
        }
        let target = if body.user_id.trim().is_empty() {
            auth.uid()
        } else {
            let parsed = Uuid::parse_str(body.user_id.trim())
                .map_err(|_| V2Error::invalid_argument("Invalid user ID."))?;
            if parsed != auth.uid() {
                // A player session may only write to its own inbox. Sending to
                // someone else is what the console route is for, where the caller
                // is an operator whose game owns both accounts.
                return Err(V2Error::permission_denied(
                    "A player may only send a notification to itself.",
                ));
            }
            parsed
        };
        let queries = self.plane.q().await?;
        queries.get_user_by_id(target).await?;
        queries
            .create_notification(
                &crate::repositories::notification::CreateNotificationParams {
                    id: Uuid::new_v4(),
                    user_id: target,
                    subject: subject.to_owned(),
                    content: if body.content.is_null() {
                        serde_json::json!({})
                    } else {
                        body.content
                    },
                    code: body.code,
                    sender_id: auth.uid(),
                },
            )
            .await?;
        Ok(Json(Empty {}))
    }
}
