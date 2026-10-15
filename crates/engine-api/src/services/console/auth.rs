//! `/v2/console/authenticate` — the console login and logout.
//!
//! Two credentials can sign in: a `console_user` row, and the
//! `[control.console]` bootstrap account that exists so a fresh database can be
//! signed into at all. The configured account wins only when its username and
//! password match, which is how it shadows a same-named row.

use axum::Json;
use axum::http::HeaderMap;
use uuid::Uuid;

use crate::app::middleware::bearer_token;
use crate::dto::Empty;
use crate::dto::console::auth::{AuthenticateRequest, ConsoleSessionResponse, LogoutRequest};
use crate::error::V2Error;
use crate::infra::console::{self, ConsoleRole};
use crate::repositories::console::InsertAuditLogParams;
use crate::ApiState;

impl super::ConsoleService {
    pub async fn authenticate(
        &self,
        state: &ApiState,
        body: AuthenticateRequest,
    ) -> Result<Json<ConsoleSessionResponse>, V2Error> {
        let username = body.username.trim();
        // One wording for every failure: a caller must not learn which half was
        // wrong, nor whether the account exists.
        if username.is_empty() || body.password.is_empty() {
            return Err(invalid_credentials());
        }

        let (id, email, role) = resolve(state, username, &body.password).await?;
        record(state, parse_id(id.as_deref()), username, &email, "authenticate.login", "Console sign-in").await;
        let token = console::mint(id.as_deref().unwrap_or(""), username, &email, role);

        Ok(Json(ConsoleSessionResponse {
            token,
            username: username.to_owned(),
            email,
            role: role.as_str(),
        }))
    }

    /// Revoke the presented console token. Always succeeds — a logout of a token
    /// that is already invalid is not an error — so a client can drop it freely.
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
            record(
                state,
                claims.uuid(),
                &claims.username,
                &claims.email,
                "authenticate.logout",
                "Console sign-out",
            )
            .await;
        }
        Ok(Json(Empty {}))
    }
}

/// A `console_user.id` as a UUID; the bootstrap admin has no row, so `None`.
fn parse_id(raw: Option<&str>) -> Option<Uuid> {
    raw.and_then(|value| Uuid::parse_str(value).ok())
}

/// Append a console action to the audit trail, best effort. An operator must not
/// be locked out — or a logout blocked — because the trail could not be written,
/// so a missing database and a failed insert are both silent here.
async fn record(
    state: &ApiState,
    id: Option<Uuid>,
    username: &str,
    email: &str,
    action: &str,
    message: &str,
) {
    let Some(queries) = state.q.clone() else {
        return;
    };
    let _ = queries
        .insert_audit_log(&InsertAuditLogParams {
            id: Uuid::new_v4(),
            // The bootstrap admin has no row; the nil id is how the trail tells
            // it apart from a real operator.
            console_user_id: id.unwrap_or_else(Uuid::nil),
            console_username: username.to_owned(),
            email: email.to_owned(),
            action: action.to_owned(),
            resource: "console".to_owned(),
            message: message.to_owned(),
            metadata: serde_json::json!({}),
        })
        .await;
}

/// The console user id (empty-able for the bootstrap account), its email, and its role.
async fn resolve(
    state: &ApiState,
    username: &str,
    password: &str,
) -> Result<(Option<String>, String, ConsoleRole), V2Error> {
    let configured = &state.control.console;
    if username == configured.username && password == configured.password {
        return Ok((None, bootstrap_email(username), ConsoleRole::Admin));
    }

    // A username that is not the bootstrap account needs the database; so does a
    // wrong password for it, which is why this is not an `else`.
    let queries = state.q.clone().ok_or_else(invalid_credentials)?;
    let Ok(row) = queries.find_console_user_auth(username).await else {
        // The same wording whether the row is missing or the database is down: a
        // login probe must not learn either.
        return Err(invalid_credentials());
    };
    if row.disable_time.timestamp() != 0 {
        return Err(invalid_credentials());
    }
    if row.mfa_required.unwrap_or(false) {
        return Err(V2Error::permission_denied(
            "This console user has MFA enabled, which is not supported yet.",
        ));
    }
    let Some(stored) = row.password.as_deref() else {
        // The row exists but has no password yet (an inviter has not set one).
        return Err(invalid_credentials());
    };
    if !crate::infra::jwt::verify_password(password, stored) {
        return Err(invalid_credentials());
    }

    Ok((
        Some(row.id.to_string()),
        row.email,
        ConsoleRole::from_acl(&row.acl),
    ))
}

fn invalid_credentials() -> V2Error {
    V2Error::unauthenticated("Invalid credentials.")
}

/// A stable placeholder email for the bootstrap account, which has no row.
fn bootstrap_email(username: &str) -> String {
    format!("{username}@console.local")
}
