use axum::Json;
use axum::http::HeaderMap;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::bearer_token;
use crate::dto::Empty;
use crate::dto::console::auth::{AuthenticateRequest, ConsoleSessionResponse, LogoutRequest};
use crate::error::V2Error;
use crate::infra::console::{self, ConsoleRole};

impl super::ConsoleService {
    pub async fn authenticate(
        &self,
        state: &ApiState,
        body: AuthenticateRequest,
    ) -> Result<Json<ConsoleSessionResponse>, V2Error> {
        let username = body.username.trim();
        if username.is_empty() || body.password.is_empty() {
            return Err(invalid_credentials());
        }

        let configured = &state.control.console;
        if username == configured.username && body.password == configured.password {
            let email = bootstrap_email(username);
            return Ok(Json(
                sign_in(state, None, username, &email, ConsoleRole::Admin).await,
            ));
        }

        let queries = super::queries(state)
            .await
            .map_err(|_| invalid_credentials())?;
        let Ok(row) = queries.find_console_user_mfa(username).await else {
            return Err(invalid_credentials());
        };
        if row.disable_time.timestamp() != 0 {
            return Err(invalid_credentials());
        }
        let Some(stored) = row.password.as_deref() else {
            return Err(invalid_credentials());
        };
        if !crate::infra::jwt::verify_password(&body.password, stored) {
            return Err(invalid_credentials());
        }

        self.verify_second_factor(&queries, &row, &body.mfa).await?;

        let role = ConsoleRole::from_acl(&row.acl);
        Ok(Json(
            sign_in(state, Some(row.id), &row.username, &row.email, role).await,
        ))
    }

    pub async fn logout(
        &self,
        state: &ApiState,
        headers: &HeaderMap,
        body: LogoutRequest,
    ) -> Result<Json<Empty>, V2Error> {
        let token = bearer_token(headers)
            .map(str::to_owned)
            .unwrap_or_else(|| body.token.clone());
        if !token.is_empty()
            && let Ok(claims) = console::verify(&token)
        {
            console::revoke(claims.uuid(), claims.exp, &claims.jti);
            super::insert_audit(
                state,
                claims.uuid(),
                &claims.username,
                &claims.email,
                "authenticate.logout",
                "console",
                claims.uuid(),
                "Signed out",
            )
            .await;
        }
        Ok(Json(Empty {}))
    }
}

async fn sign_in(
    state: &ApiState,
    id: Option<Uuid>,
    username: &str,
    email: &str,
    role: ConsoleRole,
) -> ConsoleSessionResponse {
    super::insert_audit(
        state,
        id,
        username,
        email,
        "authenticate.login",
        "console",
        id,
        &format!("Signed in as {username}"),
    )
    .await;
    let id = id.map(|id| id.to_string()).unwrap_or_default();
    ConsoleSessionResponse {
        token: console::mint(&id, username, email, role),
        username: username.to_owned(),
        email: email.to_owned(),
        role: role.as_str(),
    }
}

fn invalid_credentials() -> V2Error {
    V2Error::unauthenticated("Invalid credentials.")
}

fn bootstrap_email(username: &str) -> String {
    format!("{username}@console.local")
}
