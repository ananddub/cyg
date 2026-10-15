//! The membership rule every channel read and write shares.

use uuid::Uuid;

use crate::ApiState;
use crate::services::auth::q;
use crate::error::V2Error;

use crate::utils::channel::{MODE_DM, MODE_GROUP, Stream};

/// `group_edge` ranks: anything worse than a member (join request, ban) may
/// not read the group's channel — the `groupCheckUserPermission(.., 2)`.
const ROLE_MAX: i16 = 2;

/// Reading or writing a channel requires standing in it: a group channel is for members, a
/// DM for its two participants, and a room is open (the realtime layer decides who may join).
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
            let queries = q(state)?;
            match queries.get_group_edge_state(uid, stream.subject).await {
                Ok(state_code) if state_code <= ROLE_MAX => Ok(()),
                Ok(_) | Err(sqlx::Error::RowNotFound) => {
                    Err(V2Error::invalid_argument("Group not found."))
                }
                Err(e) => Err(V2Error::from_sqlx(e)),
            }
        }
        _ => Ok(()),
    }
}
