//! Creating, editing and deleting a console operator.

use axum::Json;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::ConsoleAuthed;
use crate::dto::Empty;
use crate::dto::console::user::{
    ConsoleUserView, CreateConsoleUserRequest, UpdateConsoleUserRequest,
};
use crate::error::V2Error;
use crate::repositories::console_mutations::{InsertConsoleUserParams, UpdateConsoleUserParams};

use super::{check_password, named, operator_conflict, role_from};

impl super::ConsoleService {
    pub async fn create_console_user(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        body: CreateConsoleUserRequest,
    ) -> Result<Json<ConsoleUserView>, V2Error> {
        claims.require_admin()?;
        let (username, email) = named(&body.username, &body.email)?;
        let role = role_from(&body.role)?;
        check_password(&body.password)?;

        let queries = super::queries(state)?;
        let id = Uuid::new_v4();
        queries
            .insert_console_user(&InsertConsoleUserParams {
                id,
                username: username.clone(),
                email,
                acl: role.to_acl(),
                password: crate::infra::jwt::hash_password(&body.password)?,
                mfa_required: body.mfa_required,
            })
            .await
            .map_err(operator_conflict)?;

        super::audit(
            state,
            claims,
            "console_user.create",
            "console_user",
            Some(id),
            &format!("Created console user {username} as {}.", role.as_str()),
        )
        .await;
        Ok(Json(self.view(&queries, id).await?))
    }

    pub async fn update_console_user(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
        body: UpdateConsoleUserRequest,
    ) -> Result<Json<ConsoleUserView>, V2Error> {
        claims.require_admin()?;
        let id = super::id(id, "console user")?;
        let (username, email) = named(&body.username, &body.email)?;
        let role = role_from(&body.role)?;

        let queries = super::queries(state)?;
        // The row is read first so an edit cannot silently become a create: the
        // update reports how many rows it touched, and this is what makes a missing
        // one a 404.
        self.view(&queries, id).await?;
        queries
            .update_console_user(&UpdateConsoleUserParams {
                id,
                username: username.clone(),
                email,
                acl: role.to_acl(),
                metadata: body.metadata.unwrap_or_else(|| serde_json::json!({})),
                mfa_required: body.mfa_required,
            })
            .await
            .map_err(operator_conflict)?;

        super::audit(
            state,
            claims,
            "console_user.update",
            "console_user",
            Some(id),
            &format!("Edited console user {username} as {}.", role.as_str()),
        )
        .await;
        Ok(Json(self.view(&queries, id).await?))
    }

    pub async fn delete_console_user(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        id: &str,
    ) -> Result<Json<Empty>, V2Error> {
        claims.require_admin()?;
        let id = super::id(id, "console user")?;
        self.must_be_another(claims, id, "delete")?;

        let queries = super::queries(state)?;
        let view = self.view(&queries, id).await?;
        queries.delete_console_user(id).await?;
        super::audit(
            state,
            claims,
            "console_user.delete",
            "console_user",
            Some(id),
            &format!("Deleted console user {}.", view.username),
        )
        .await;
        Ok(Json(Empty {}))
    }
}
