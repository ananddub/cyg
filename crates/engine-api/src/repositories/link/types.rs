use super::*;

pub struct LinkEmailParams {
    pub email: String,
    pub d: Vec<u8>,
    pub id: Uuid,
}

pub struct LinkGoogleParams {
    pub google_id: String,
    pub param3: String,
    pub param4: String,
    pub id: Uuid,
}

pub struct LinkFacebookParams {
    pub facebook_id: String,
    pub param3: String,
    pub param4: String,
    pub id: Uuid,
}

pub type UpsertProviderIdentityParams = InsertProviderDeviceParams;

pub(super) async fn update_returning_id(
    queries: &Queries,
    surql: &str,
    params: BTreeMap<String, Value>,
) -> Result<Uuid, DbError> {
    match queries.one::<Users>(surql, params).await {
        Ok(user) => Ok(user.id),
        Err(DbError::Unique(_)) => Err(DbError::NotFound),
        Err(err) => Err(err),
    }
}

pub(super) use crate::repositories::user::new_device;
