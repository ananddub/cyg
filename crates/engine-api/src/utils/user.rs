//! User surface helpers: the `apiUser` view other players see.

use crate::dto::user::UserResponse;
use crate::error::V2Error;
use crate::repositories::models::Users;

/// A disabled account has no public view, so a lookup miss and a disabled account
/// give the same answer: nothing.
pub(crate) fn public_view(user: &Users) -> Option<UserResponse> {
    (user.disable_time.timestamp() <= 0).then(|| UserResponse::from_row(user))
}

/// A miss is not an error: the answer is the rows that were found.
pub(crate) fn push_found(
    users: &mut Vec<UserResponse>,
    found: Result<Users, sqlx::Error>,
) -> Result<(), V2Error> {
    match found {
        Ok(user) => {
            if let Some(view) = public_view(&user) {
                users.push(view);
            }
            Ok(())
        }
        Err(sqlx::Error::RowNotFound) => Ok(()),
        Err(e) => Err(V2Error::from_sqlx(e)),
    }
}
