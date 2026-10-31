use axum::Json;

use crate::ApiState;
use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::error::V2Error;

impl super::ConsoleService {
    pub async fn clear_console_user_mfa(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_admin()?;
        let id = super::id(id, "console user")?;
        let queries = super::queries(state).await?;
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

    pub async fn set_console_user_mfa_required(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
        required: bool,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_admin()?;
        let id = super::id(id, "console user")?;
        let queries = super::queries(state).await?;
        let view = self.view(&queries, id).await?;
        queries
            .set_console_user_mfa_required(id, required)
            .await
            .map_err(V2Error::from)?;
        let (action, verb) = if required {
            ("console_user.mfa.require", "Required MFA for")
        } else {
            (
                "console_user.mfa.unrequire",
                "Cleared the MFA requirement for",
            )
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

    pub async fn reset_console_user_mfa(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_admin()?;
        let id = super::id(id, "console user")?;
        let queries = super::queries(state).await?;
        let view = self.view(&queries, id).await?;
        queries
            .reset_console_user_mfa_factor(id)
            .await
            .map_err(V2Error::from)?;
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
