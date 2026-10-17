//! `/v2/console/user` — the console operators themselves: the ones an admin
//! creates, edits, disables and whose second factor an admin can clear.
//!
//! Identity and role travel inside the console token, so a change here takes effect
//! at the operator's next sign-in (and at logout for the token they hold now): the
//! sign-in is the revocation boundary, and `[control.console].token_expiry_secs`
//! bounds how long one can last.

mod password;
mod write;

use axum::Json;
use uuid::Uuid;

use crate::app::middleware::ConsoleAuthed;
use crate::dto::console::Page;
use crate::dto::console::user::ConsoleUserView;
use crate::error::V2Error;
use crate::infra::console::ConsoleRole;
use crate::repositories::db::Queries;
use crate::ApiState;

pub(super) use super::{ConsoleService, audit, id, never, queries};

/// The shortest password the console accepts. A console credential reaches every
/// account on the server, so it is held to more than a player's is.
const MIN_PASSWORD_LEN: usize = 12;

impl super::ConsoleService {
    pub async fn list_console_users(
        &self,
        state: &ApiState,
        page: Page,
    ) -> Result<Json<Vec<ConsoleUserView>>, V2Error> {
        let rows = super::queries(state)?
            .list_console_users(page.limit(), page.offset())
            .await?;
        Ok(Json(rows.into_iter().map(ConsoleUserView::from).collect()))
    }

    /// One operator's row, or a 404.
    async fn view(&self, queries: &Queries, id: Uuid) -> Result<ConsoleUserView, V2Error> {
        let row = queries
            .find_console_user_by_id(id)
            .await
            .map_err(|_| V2Error::not_found("Console user not found."))?;
        Ok(ConsoleUserView::from(row))
    }

    /// Refuse an operator turning an operation on itself. A token outlives the row
    /// it names, so self-deletion would revoke nothing while leaving the console one
    /// admin short — and self-disabling would lock the caller out on the next login.
    fn must_be_another(
        &self,
        claims: &ConsoleAuthed,
        id: Uuid,
        verb: &str,
    ) -> Result<(), V2Error> {
        if claims.0.uuid() == Some(id) {
            return Err(V2Error::failed_precondition(format!(
                "A console user cannot {verb} itself."
            )));
        }
        Ok(())
    }
}

/// The username and email an operator body must carry.
fn named(username: &str, email: &str) -> Result<(String, String), V2Error> {
    let username = username.trim();
    let email = email.trim();
    if username.is_empty() || email.is_empty() {
        return Err(V2Error::invalid_argument(
            "A console user needs a username and an email.",
        ));
    }
    Ok((username.to_owned(), email.to_owned()))
}

/// The role an operator named on the wire.
fn role_from(name: &str) -> Result<ConsoleRole, V2Error> {
    ConsoleRole::from_name(name.trim()).ok_or_else(|| {
        V2Error::invalid_argument("The role must be admin, developer, maintainer or readonly.")
    })
}

/// A console credential is the whole server's, so it is longer than a player's is
/// held to and may not be blank.
fn check_password(password: &str) -> Result<(), V2Error> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(V2Error::invalid_argument(format!(
            "A console password must be at least {MIN_PASSWORD_LEN} characters."
        )));
    }
    Ok(())
}

/// Name the clash: the two unique constraints on `console_user` are the only thing
/// that says whether it was the username or the email.
fn operator_conflict(e: sqlx::Error) -> V2Error {
    if V2Error::is_unique_violation(&e, "console_user_username_uniq") {
        V2Error::already_exists("That username is taken.")
    } else if V2Error::is_unique_violation(&e, "console_user_email_uniq") {
        V2Error::already_exists("That email is taken.")
    } else {
        V2Error::from_sqlx(e)
    }
}
