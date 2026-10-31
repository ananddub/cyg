use super::*;

pub async fn device(
    state: &ApiState,
    id: &str,
    username: &str,
    create: bool,
    vars: HashMap<String, String>,
) -> Result<Json<SessionResponse>, V2Error> {
    let queries = q(state).await?;
    match queries.get_device_by_id(id).await {
        Ok(row) => {
            if !row.provider.is_empty() {
                return Err(V2Error::permission_denied(
                    "User account uses provider-linked identifier.",
                ));
            }
            let user = queries
                .get_user_by_id(row.user_id)
                .await
                .map_err(lookup_failed)?;
            let user = existing(user)?;
            Ok(session(user.id, &user.username, false, vars))
        }
        Err(DbError::NotFound) if create => {
            if !crate::moderation::is_valid_username(username) {
                return Err(V2Error::invalid_argument(
                    "Username must be 3-20 characters and free of profanity.",
                ));
            }
            let uid = create_device(state, id, username).await?;
            crate::services::games::tenant::stamp_user(state, uid).await;
            Ok(session(uid, username, true, vars))
        }
        Err(DbError::NotFound) => Err(V2Error::not_found("User account not found.")),
        Err(e) => Err(lookup_failed(e)),
    }
}
pub async fn custom(
    state: &ApiState,
    id: &str,
    username: &str,
    create: bool,
    vars: HashMap<String, String>,
) -> Result<Json<SessionResponse>, V2Error> {
    let queries = q(state).await?;
    match queries.get_user_by_custom_id(id).await {
        Ok(user) => {
            let user = existing(user)?;
            Ok(session(user.id, &user.username, false, vars))
        }
        Err(DbError::NotFound) if create => {
            let user = queries
                .create_user_with_custom_id(&CreateUserWithCustomIdParams {
                    id: Uuid::new_v4(),
                    username: username.to_owned(),
                    custom_id: id.to_owned(),
                })
                .await
                .map_err(insert_failed)?;
            crate::services::games::tenant::stamp_user(state, user.id).await;
            Ok(session(user.id, &user.username, true, vars))
        }
        Err(DbError::NotFound) => Err(V2Error::not_found("User account not found.")),
        Err(e) => Err(lookup_failed(e)),
    }
}
pub async fn email(
    state: &ApiState,
    login: Login,
    create: bool,
    vars: HashMap<String, String>,
) -> Result<Json<SessionResponse>, V2Error> {
    let queries = q(state).await?;
    match login {
        Login::Email {
            email,
            password,
            username,
        } => match queries.get_user_by_email(&email).await {
            Ok(user) => {
                let user = existing(user)?;
                check_password(&password, &user.password)?;
                Ok(session(user.id, &user.username, false, vars))
            }
            Err(DbError::NotFound) if create => {
                let user = queries
                    .create_user_with_email(&CreateUserWithEmailParams {
                        id: Uuid::new_v4(),
                        username,
                        email,
                        password: jwt::hash_password(&password)?,
                    })
                    .await
                    .map_err(insert_failed)?;
                crate::services::games::tenant::stamp_user(state, user.id).await;
                Ok(session(user.id, &user.username, true, vars))
            }
            Err(DbError::NotFound) => Err(V2Error::not_found("User account not found.")),
            Err(e) => Err(lookup_failed(e)),
        },
        Login::Username { username, password } => {
            let user = match queries.get_user_by_username(&username).await {
                Ok(user) => existing(user)?,
                Err(DbError::NotFound) => {
                    return Err(V2Error::not_found("User account not found."));
                }
                Err(e) => return Err(lookup_failed(e)),
            };
            check_password(&password, &user.password)?;
            Ok(session(user.id, &user.username, false, vars))
        }
    }
}
fn check_password(password: &str, stored: &Option<Vec<u8>>) -> Result<(), V2Error> {
    if !jwt::verify_password(password, stored.as_deref().unwrap_or_default()) {
        return Err(V2Error::unauthenticated("Invalid credentials."));
    }
    Ok(())
}
