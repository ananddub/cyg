//! An operator's credential and its revocation: the password reset, the disable and
//! enable, and the admin clear of a second factor.

use axum::Json;

use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::console::user::SetConsolePasswordRequest;
use crate::error::V2Error;
use crate::ApiState;

use super::check_password;

impl super::ConsoleService {
    pub async fn set_console_user_password(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
        body: SetConsolePasswordRequest,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_admin()?;
        let id = super::id(id, "console user")?;
        check_password(&body.password)?;

        let queries = super::queries(state)?;
        let view = self.view(&queries, id).await?;
        queries
            .update_console_user_password(&crate::infra::jwt::hash_password(&body.password)?, id)
            .await?;
        super::audit(
            state,
            claims,
            "console_user.password",
            "console_user",
            Some(id),
            &format!("Reset the password of {}.", view.username),
        )
        .await;
        Ok(Json(Empty {}))
    }

    /// Disable or enable an operator. The epoch means enabled, as it does for a
    /// player and for a group that was never deleted.
    pub async fn set_console_user_disabled(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
        disabled: bool,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_admin()?;
        let id = super::id(id, "console user")?;
        if disabled {
            self.must_be_another(claims, id, "disable")?;
        }

        let queries = super::queries(state)?;
        let view = self.view(&queries, id).await?;
        let at = if disabled {
            chrono::Utc::now()
        } else {
            super::never()
        };
        queries.set_console_user_disabled(at, id).await?;

        let (action, verb) = if disabled {
            ("console_user.disable", "Disabled")
        } else {
            ("console_user.enable", "Enabled")
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
}
