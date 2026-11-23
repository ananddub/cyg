use axum::Json;
use uuid::Uuid;

use crate::app::middleware::Authed;
use crate::dto::Empty;
use crate::error::V2Error;

impl super::AccountService {
    pub async fn delete_account(&self, auth: &Authed) -> Result<Json<Empty>, V2Error> {
        self.erase(auth.uid()).await
    }

    /// Erases an account by id and immediately invalidates everything it had
    /// live. Used both by the player's own `DELETE /v1/account` and by the
    /// service-driven `POST /v1/account/delete`, so the two paths cannot drift.
    pub async fn erase(&self, uid: Uuid) -> Result<Json<Empty>, V2Error> {
        let queries = self.plane.q().await?;
        // The tombstone is written inside the erase transaction itself, so a
        // failure rolls the whole deletion (tombstone included) back.
        queries.delete_account(uid).await?;

        // A deleted account must not keep a live session or a connected room.
        self.plane.sessions.remove_all(uid);
        crate::services::realtime::RealtimeSessionHub::global()
            .kick_user(uid, "This account has been deleted.");

        crate::services::webhook::emit(
            &self.plane,
            "account.deleted",
            &uid.to_string(),
            serde_json::json!({ "user_id": uid, "method": "self" }),
        )
        .await;
        Ok(Json(Empty {}))
    }
}
