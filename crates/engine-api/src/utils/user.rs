use crate::db::DbError;
use crate::dto::user::UserResponse;
use crate::error::V2Error;
use crate::repositories::models::Users;

pub(crate) fn public_view(user: &Users) -> Option<UserResponse> {
    (user.disable_time.timestamp() <= 0).then(|| UserResponse::from_row(user))
}

pub(crate) fn push_found(
    users: &mut Vec<UserResponse>,
    found: Result<Users, DbError>,
) -> Result<(), V2Error> {
    match found {
        Ok(user) => {
            if let Some(view) = public_view(&user) {
                users.push(view);
            }
            Ok(())
        }
        Err(DbError::NotFound) => Ok(()),
        Err(e) => Err(V2Error::from(e)),
    }
}
