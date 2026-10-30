use axum::Json;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::Empty;
use crate::error::V2Error;

impl super::AccountService {
    pub async fn delete_account(
        &self,
        state: &ApiState,
        auth: &Authed,
    ) -> Result<Json<Empty>, V2Error> {
        self.erase(state, auth.uid()).await
    }

    /// Erases an account by id and immediately invalidates everything it had
    /// live. Used both by the player's own `DELETE /v2/account` and by the
    /// service-driven `POST /v2/account/delete`, so the two paths cannot drift.
    pub async fn erase(&self, state: &ApiState, uid: Uuid) -> Result<Json<Empty>, V2Error> {
        let queries = crate::services::deps::q(state).await?;
        // The tombstone is written inside the erase transaction itself, so a
        // failure rolls the whole deletion (tombstone included) back.
        queries.delete_account(uid).await?;

        // A deleted account must not keep a live session or a connected room.
        state.sessions.remove_all(uid);
        crate::services::realtime::RealtimeSessionHub::global()
            .kick_user(uid, "This account has been deleted.");
        Ok(Json(Empty {}))
    }
}
