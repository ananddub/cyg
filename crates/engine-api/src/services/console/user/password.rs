use axum::Json;

use crate::ApiState;
use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::console::user::{ChangeConsolePasswordRequest, SetConsolePasswordRequest};
use crate::error::V2Error;
use crate::services::deps::q;

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

        let queries = q(state).await?;
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

        let queries = q(state).await?;
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

    pub async fn change_console_password(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        body: ChangeConsolePasswordRequest,
    ) -> Result<Json<Empty>, V2Error> {
        check_password(&body.password)?;
        let Some(id) = claims.0.uuid() else {
            return Err(V2Error::failed_precondition(
                "The bootstrap console account has no password to change.",
            ));
        };
        let queries = q(state).await?;
        let row = queries
            .find_console_user_mfa_by_id(id)
            .await
            .map_err(|_| V2Error::not_found("Console user not found."))?;
        let stored = row
            .password
            .as_deref()
            .ok_or_else(|| V2Error::failed_precondition("No password is set for this account."))?;
        if !crate::infra::jwt::verify_password(&body.current_password, stored) {
            return Err(V2Error::unauthenticated("Current password is incorrect."));
        }
        queries
            .update_console_user_password(&crate::infra::jwt::hash_password(&body.password)?, id)
            .await
            .map_err(V2Error::from)?;
        super::audit(
            state,
            claims,
            "console_user.password_change",
            "console_user",
            Some(id),
            &format!("Changed their own password ({})", row.username),
        )
        .await;
        Ok(Json(Empty {}))
    }
}
