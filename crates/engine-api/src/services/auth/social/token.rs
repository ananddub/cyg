use std::collections::HashMap;

use axum::Json;
use uuid::Uuid;

use super::super::validate;
use super::super::verify::{self, Kind, Profile};
use crate::ApiState;
use crate::db::DbError;
use crate::dto::session::SessionResponse;
use crate::error::V2Error;
use crate::repositories::auth::{
    CreateUserWithAppleParams, CreateUserWithFacebookParams, CreateUserWithGoogleParams,
    CreateUserWithSteamParams,
};
use crate::repositories::db::Queries;
use crate::repositories::models::Users;
use crate::services::deps::q;
use crate::utils::auth::{existing, insert_failed, lookup_failed, session};

pub(crate) async fn token(
    state: &ApiState,
    kind: Kind,
    raw: &str,
    requested_username: &str,
    create: bool,
    vars: HashMap<String, String>,
) -> Result<Json<SessionResponse>, V2Error> {
    let username = validate::username(requested_username)?;
    if matches!(kind, Kind::Facebook | Kind::Google) && raw.is_empty() {
        return Err(V2Error::invalid_argument(format!(
            "{} access token is required.",
            kind.label()
        )));
    }
    let profile = verify::profile(&state.control.social, kind, raw).await?;
    let queries = q(state).await?;
    let found = match kind {
        Kind::Apple => queries.get_user_by_apple_id(&profile.id).await,
        Kind::Facebook => queries.get_user_by_facebook_id(&profile.id).await,
        Kind::Google => queries.get_user_by_google_id(&profile.id).await,
        Kind::Steam => queries.get_user_by_steam_id(&profile.id).await,
    };
    match found {
        Ok(user) => {
            let user = existing(user)?;
            Ok(session(user.id, &user.username, false, vars))
        }
        Err(DbError::NotFound) if create => {
            if !crate::moderation::is_valid_username(&username) {
                return Err(V2Error::invalid_argument(
                    "Username must be 3-20 characters and free of profanity.",
                ));
            }
            let user = create_user(&queries, kind, &profile, &username)
                .await
                .map_err(insert_failed)?;
            crate::services::games::tenant::stamp_user(state, user.id).await;
            crate::services::webhook::emit(
                state,
                "account.created",
                &user.id.to_string(),
                serde_json::json!({
                    "user_id": user.id,
                    "username": user.username,
                    "method": kind.label(),
                }),
            )
            .await;
            Ok(session(user.id, &user.username, true, vars))
        }
        Err(DbError::NotFound) => Err(V2Error::not_found("User account not found.")),
        Err(e) => Err(lookup_failed(e)),
    }
}

async fn create_user(
    queries: &Queries,
    kind: Kind,
    profile: &Profile,
    username: &str,
) -> Result<Users, DbError> {
    let id = Uuid::new_v4();
    let display_name = profile
        .display_name
        .clone()
        .unwrap_or_else(|| username.to_owned());
    let avatar_url = profile.avatar_url.clone().unwrap_or_default();
    match kind {
        Kind::Apple => {
            queries
                .create_user_with_apple(&CreateUserWithAppleParams {
                    id,
                    username: username.to_owned(),
                    apple_id: profile.id.clone(),
                })
                .await
        }
        Kind::Steam => {
            queries
                .create_user_with_steam(&CreateUserWithSteamParams {
                    id,
                    username: username.to_owned(),
                    steam_id: profile.id.clone(),
                })
                .await
        }
        Kind::Facebook => {
            queries
                .create_user_with_facebook(&CreateUserWithFacebookParams {
                    id,
                    username: username.to_owned(),
                    display_name,
                    avatar_url,
                    facebook_id: profile.id.clone(),
                })
                .await
        }
        Kind::Google => {
            queries
                .create_user_with_google(&CreateUserWithGoogleParams {
                    id,
                    username: username.to_owned(),
                    google_id: profile.id.clone(),
                    display_name,
                    avatar_url,
                })
                .await
        }
    }
}
