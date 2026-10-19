//! Realtime Notification Service singleton.

use std::sync::Arc;
use auto_di::singleton;
use auto_ws::{Conn, Msg};

use crate::ApiState;
use crate::error::V2Error;
use crate::infra::ws::envelope::EnvelopeMessage;
use crate::infra::ws::session;

use crate::controllers::realtime::messages::*;
use crate::controllers::realtime::{Dispatched, session_of, state_of};

pub struct RealtimeNotificationService;

#[singleton]
impl RealtimeNotificationService {
    pub fn new() -> Self {
        Self
    }
}

impl RealtimeNotificationService {
    pub async fn list(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: NotificationsList = msg.payload()?;
        self.query(&state_of(conn), &session_of(conn), req.limit, req.cacheable_cursor).await
    }

    /// Socket-side notification pull; shares the query the REST surface uses.
    async fn query(
        &self,
        state: &ApiState,
        session: &Arc<session::Session>,
        limit: Option<i64>,
        cacheable_cursor: Option<String>,
    ) -> Dispatched {
        let queries = crate::services::auth::q(state)?;
        let rows = queries
            .list_notifications(&crate::repositories::notification::ListNotificationsParams {
                user_id: session.uid,
                create_time: chrono::Utc::now(),
                limit: limit.unwrap_or(50).clamp(1, 100),
            })
            .await
            .map_err(V2Error::from_sqlx)?;
        let notifications = rows
            .iter()
            .map(crate::dto::notification::NotificationResponse::from)
            .collect();
        Ok(Some(EnvelopeMessage::Notifications {
            notifications,
            cacheable_cursor: cacheable_cursor.unwrap_or_default(),
        }))
    }
}
