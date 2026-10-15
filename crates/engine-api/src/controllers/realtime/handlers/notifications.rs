//! `notifications`.

use std::sync::Arc;

use auto_ws::{Conn, Msg};

use crate::ApiState;
use crate::error::V2Error;
use crate::infra::ws::envelope::EnvelopeMessage;
use crate::infra::ws::session;

use super::super::messages::*;
use super::super::{Dispatched, session_of, state_of};

pub(crate) async fn list(conn: &Conn, msg: &Msg) -> Dispatched {
    let req: NotificationsList = msg.payload()?;
    query(&state_of(conn), &session_of(conn), req.limit, req.cacheable_cursor).await
}

/// Socket-side notification pull; shares the query the REST surface uses.
async fn query(
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
