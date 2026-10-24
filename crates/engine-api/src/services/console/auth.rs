//! `/v2/console/authenticate` — the console login and logout.
//!
//! Two credentials can sign in: a `console_user` row, and the
//! `[control.console]` bootstrap account that exists so a fresh database can be
//! signed into at all. The configured account wins only when its username and
//! password match, which is how it shadows a same-named row. A row with a second
//! factor enrolled needs one more field before a token is minted.

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
        // One wording for every failure: a caller must not learn which half was
        // wrong, nor whether the account exists.
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

        // A username that is not the bootstrap account needs the database; so does a
        // wrong password for it, which is why this is not an `else`.
        let queries = state.q.clone().ok_or_else(invalid_credentials)?;
        let Ok(row) = queries.find_console_user_mfa(username).await else {
            // The same wording whether the row is missing or the database is down:
            // a login probe must not learn either.
            return Err(invalid_credentials());
        };
        if row.disable_time.timestamp() != 0 {
            return Err(invalid_credentials());
        }
        let Some(stored) = row.password.as_deref() else {
            // The row exists but has no password yet (nobody has set one), or the
            // MFA assertion below is all it has.
            return Err(invalid_credentials());
        };
        if !crate::infra::jwt::verify_password(&body.password, stored) {
            return Err(invalid_credentials());
        }

        // Past the password, the second factor decides. It is checked before the
        // audit entry, so an attempt that never completed leaves no trail claiming
        // it signed in.
        self.verify_second_factor(&queries, &row, &body.mfa).await?;

        let role = ConsoleRole::from_acl(&row.acl);
        Ok(Json(
            sign_in(state, Some(row.id), &row.username, &row.email, role).await,
        ))
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

/// Mint the token and record the sign-in. The two belong together: a session that
/// was minted but not recorded, or recorded but not minted, would be a trail that
/// disagrees with the tokens in circulation.
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

/// A stable placeholder email for the bootstrap account, which has no row.
fn bootstrap_email(username: &str) -> String {
    format!("{username}@console.local")
}
