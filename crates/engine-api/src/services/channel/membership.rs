use uuid::Uuid;

use crate::ApiState;
use crate::db::DbError;
use crate::error::V2Error;
use crate::services::deps::q;

use crate::utils::channel::{MODE_DM, MODE_GROUP, Stream};

const ROLE_MAX: i16 = 2;

pub(crate) async fn assert_member(
    state: &ApiState,
    uid: Uuid,
    stream: &Stream,
) -> Result<(), V2Error> {
    match stream.mode {
        MODE_DM => {
            if uid != stream.subject && uid != stream.descriptor {
                return Err(V2Error::invalid_argument("Channel not found."));
            }
            Ok(())
        }
        MODE_GROUP => {
            let queries = q(state).await?;
            match queries.get_group_edge_state(uid, stream.subject).await {
                Ok(state_code) if state_code <= ROLE_MAX => Ok(()),
                Ok(_) | Err(DbError::NotFound) => {
                    Err(V2Error::invalid_argument("Group not found."))
                }
                Err(e) => Err(V2Error::from(e)),
            }
        }
        _ => Ok(()),
    }
}
