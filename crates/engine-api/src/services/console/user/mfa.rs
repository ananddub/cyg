//! An admin's control over another operator's second factor: the clear, the force
//! on or off of the requirement, and the reset of the enrolled factor.

use axum::Json;

use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::error::V2Error;
use crate::ApiState;

use super::{manual, pool};

impl super::ConsoleService {
    /// Clear another operator's second factor. An admin does this when someone has
    /// lost both their device and their recovery codes, so no code can be demanded —
    /// which is exactly why this is the one MFA route that is admin-only.
    pub async fn clear_console_user_mfa(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_admin()?;
        let id = super::id(id, "console user")?;
        let queries = super::queries(state)?;
        let view = self.view(&queries, id).await?;
        queries.clear_console_user_mfa(id).await?;
        super::audit(
            state,
            claims,
            "console_user.mfa.clear",
            "console_user",
            Some(id),
            &format!("Cleared the MFA of {}.", view.username),
        )
        .await;
        Ok(Json(Empty {}))
    }

    /// Force another operator's second-factor requirement on or off, without
    /// touching what they have enrolled. Mirroring the console's MFA-require handler:
    /// it writes only the `mfa_required` flag, so an operator already enrolled keeps
    /// their secret, and one not yet enrolled is told to enrol at their next sign-in.
    pub async fn set_console_user_mfa_required(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
        required: bool,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_admin()?;
        let id = super::id(id, "console user")?;
        let queries = super::queries(state)?;
        let view = self.view(&queries, id).await?;
        manual::set_mfa_required(pool(state)?, id, required)
            .await
            .map_err(V2Error::from_sqlx)?;
        let (action, verb) = if required {
            ("console_user.mfa.require", "Required MFA for")
        } else {
            ("console_user.mfa.unrequire", "Cleared the MFA requirement for")
        };
        super::audit(
            state,
            claims,
            action,
            "console_user",
            Some(id),
            &format!("{verb} {}.", view.username),
        )
        .await;
        Ok(Json(Empty {}))
    }

    /// Reset another operator's second factor: clear the enrolled secret and its
    /// recovery codes but keep the `mfa_required` flag, so the account is still held
    /// to having one and re-enrols at its next sign-in. This is the console's reset,
    /// distinct from a delete that would also drop the requirement.
    pub async fn reset_console_user_mfa(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_admin()?;
        let id = super::id(id, "console user")?;
        let queries = super::queries(state)?;
        let view = self.view(&queries, id).await?;
        manual::reset_mfa_factor(pool(state)?, id)
            .await
            .map_err(V2Error::from_sqlx)?;
        super::audit(
            state,
            claims,
            "console_user.mfa.reset",
            "console_user",
            Some(id),
            &format!("Reset the MFA of {}.", view.username),
        )
        .await;
        Ok(Json(Empty {}))
    }
}
