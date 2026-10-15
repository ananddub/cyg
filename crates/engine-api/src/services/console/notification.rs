//! `/v2/console/notification` — a player's notifications.

use axum::Json;

use crate::dto::console::notification::{NotificationQuery, NotificationRow};
use crate::error::V2Error;
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
}
