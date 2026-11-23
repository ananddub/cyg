use crate::services::plane::Plane;
use auto_di::singleton;
use auto_ws::{Conn, Msg};
use std::sync::Arc;

use crate::error::V2Error;
use crate::infra::ws::envelope::EnvelopeMessage;
use crate::infra::ws::session;

use crate::controllers::realtime::messages::*;
use crate::controllers::realtime::{Dispatched, session_of};

pub struct RealtimeNotificationService {
    pub(crate) plane: Arc<Plane>,
}

#[singleton]
impl RealtimeNotificationService {
    pub fn new(plane: Arc<Plane>) -> Self {
        Self { plane }
    }
}

impl RealtimeNotificationService {
    pub async fn list(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: NotificationsList = msg.payload()?;
        self.query(&session_of(conn), req.limit, req.cacheable_cursor)
            .await
    }

    async fn query(
        &self,

        session: &Arc<session::Session>,
        limit: Option<i64>,
        cacheable_cursor: Option<String>,
    ) -> Dispatched {
        let queries = self.plane.q().await?;
        let rows = queries
            .list_notifications(
                &crate::repositories::notification::ListNotificationsParams {
                    user_id: session.uid,
                    create_time: chrono::Utc::now(),
                    limit: limit.unwrap_or(50).clamp(1, 100),
                },
            )
            .await
            .map_err(V2Error::from)?;
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
