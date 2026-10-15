//! Group surface helpers: id parsing, metadata decoding and the row→model projection
//! that lets `apiUserGroup` reuse the group shape.

use serde_json::Value;
use uuid::Uuid;

use crate::error::V2Error;
use crate::repositories::group::ListGroupsForUserPagedRow;
use crate::repositories::models::Groups;

pub const ROLE_SUPERADMIN: i16 = 0;
pub const ROLE_ADMIN: i16 = 1;
pub const ROLE_MEMBER: i16 = 2;
pub const ROLE_JOIN_REQUEST: i16 = 3;
pub const ROLE_BANNED: i16 = 4;

pub(crate) fn group_id(raw: &str) -> Result<Uuid, V2Error> {
    Uuid::parse_str(raw)
        .ok()
        .filter(|id| !id.is_nil())
        .ok_or_else(|| V2Error::invalid_argument("Group ID must be a valid ID."))
}

pub(crate) fn metadata_of(raw: Option<&String>) -> Result<Value, V2Error> {
    match raw {
        None => Ok(Value::Object(Default::default())),
        Some(text) if text.is_empty() => Ok(Value::Object(Default::default())),
        Some(text) => serde_json::from_str::<Value>(text)
            .map_err(|_| V2Error::invalid_argument("Failed to unmarshal group metadata.")),
    }
}

/// Project a `list_groups_for_user_paged` row onto `Groups` so the user-group edge can
/// reuse [`crate::dto::group::GroupResponse`].
pub(crate) fn as_groups(row: &ListGroupsForUserPagedRow) -> Groups {
    Groups {
        id: row.id,
        creator_id: row.creator_id,
        name: row.name.clone(),
        description: row.description.clone(),
        avatar_url: row.avatar_url.clone(),
        lang_tag: row.lang_tag.clone(),
        metadata: row.metadata.clone(),
        state: row.state,
        edge_count: row.edge_count,
        max_count: row.max_count,
        create_time: row.create_time,
        update_time: row.update_time,
        disable_time: row.disable_time,
    }
}
