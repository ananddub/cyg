mod mfa;
mod password;
mod write;

use axum::Json;
use uuid::Uuid;

use crate::ApiState;
use crate::app::middleware::ConsoleAuthed;
use crate::dto::console::Page;
use crate::dto::console::user::ConsoleUserView;
use crate::error::V2Error;
use crate::infra::console::ConsoleRole;
use crate::repositories::db::Queries;
use crate::services::deps::q;

pub(super) use super::{ConsoleService, audit, id, never};

const MIN_PASSWORD_LEN: usize = 12;

impl super::ConsoleService {
    pub async fn list_console_users(
        &self,
        state: &ApiState,
        page: Page,
    ) -> Result<Json<Vec<ConsoleUserView>>, V2Error> {
        let rows = q(state)
            .await?
            .list_console_users(page.limit(), page.offset())
            .await?;
        Ok(Json(rows.into_iter().map(ConsoleUserView::from).collect()))
    }

    async fn view(&self, queries: &Queries, id: Uuid) -> Result<ConsoleUserView, V2Error> {
        let row = queries
            .find_console_user_by_id(id)
            .await
            .map_err(|_| V2Error::not_found("Console user not found."))?;
        Ok(ConsoleUserView::from(row))
    }

    fn must_be_another(&self, claims: &ConsoleAuthed, id: Uuid, verb: &str) -> Result<(), V2Error> {
        if claims.0.uuid() == Some(id) {
            return Err(V2Error::failed_precondition(format!(
                "A console user cannot {verb} itself."
            )));
        }
        Ok(())
    }
}

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

fn role_from(name: &str) -> Result<ConsoleRole, V2Error> {
    ConsoleRole::from_name(name.trim()).ok_or_else(|| {
        V2Error::invalid_argument("The role must be admin, developer, maintainer or readonly.")
    })
}

fn check_password(password: &str) -> Result<(), V2Error> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(V2Error::invalid_argument(format!(
            "A console password must be at least {MIN_PASSWORD_LEN} characters."
        )));
    }
    Ok(())
}

fn operator_conflict(e: crate::db::DbError) -> V2Error {
    if V2Error::is_unique_violation(&e, "console_user_username_uniq") {
        V2Error::already_exists("That username is taken.")
    } else if V2Error::is_unique_violation(&e, "console_user_email_uniq") {
        V2Error::already_exists("That email is taken.")
    } else {
        V2Error::from(e)
    }
}
