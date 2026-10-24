//! Notifications (`/v2/notification*`).
//!
//! BaaS parity: `server/api_notification.go`, `core_notification.go`. The client surface is
//! read + delete, and pages go *forward*: `cacheableCursor` is the newest row's `(create_time, id)`.

use auto_di::singleton;
use axum::Json;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::Empty;
use crate::dto::notification::{
    DeleteRequest, ListQuery, NotificationListResponse, NotificationResponse,
};
use crate::error::V2Error;
use crate::repositories::notification::{
    ListNotificationsByCodePagedParams, ListNotificationsBySenderPagedParams,
    ListNotificationsPagedParams,
};
use crate::services::auth::q;
use crate::services::social::ts_head;

use crate::utils::notification::{
    LIMIT_RANGE, decode_cursor, encode_cursor, encode_cursor_at, flatten,
};

pub struct NotificationService;

#[singleton]
impl NotificationService {
    fn new() -> Self {
        Self
    }
}

impl NotificationService {
    /// `GET /v2/notification?limit=&cacheableCursor=` — the
    /// `ListNotifications`.
    pub async fn list(
        &self,
        state: &ApiState,
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
        let queries = q(state)?;
        let uid = claims.uid();

        // One row past the limit is the "a page follows" signal; the extra row
        // is dropped and the cursor resumes at the last row kept.
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
        // An empty page renews the cursor it was given, or offers an empty one.
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

    /// `DELETE /v2/notification?ids=…` — the `DeleteNotifications`, which
    /// deletes exactly the ids named and does nothing when the list is empty.
    pub async fn delete_many(
        &self,
        state: &ApiState,
        auth: &Authed,
        req: DeleteRequest,
    ) -> Result<Json<Empty>, V2Error> {
        let uid = auth.0.uuid()?;
        let queries = q(state)?;
        for id in flatten(req.ids) {
            delete_one(&queries, uid, id).await?;
        }
        Ok(Json(Empty {}))
    }

    /// `DELETE /v2/notification/{id}` — the single-row form.
    pub async fn delete_one(
        &self,
        state: &ApiState,
        auth: &Authed,
        id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        let id = Uuid::parse_str(id)
            .map_err(|_| V2Error::invalid_argument("Invalid notification ID."))?;
        let queries = q(state)?;
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
            sqlx::Error::RowNotFound => V2Error::not_found("Notification not found."),
            other => V2Error::from_sqlx(other),
        })?;
    Ok(())
}
