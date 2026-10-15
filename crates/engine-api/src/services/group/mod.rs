//! Groups (`/v2/group*`).
//!
//! BaaS parity: `server/api_group.go`, `server/core_group.go`. Membership is `group_edge`
//! (`0=Superadmin, 1=Admin, 2=Member, 3=JoinRequest, 4=Banned`); a closed group's join leaves a request that takes no slot.

use uuid::Uuid;

use crate::repositories::db::Queries;
use crate::repositories::models::Groups;
use crate::error::V2Error;

pub mod groups;
pub mod members;
pub mod user_groups;

pub use groups::GroupService;
pub use members::GroupMemberService;
pub use user_groups::UserGroupService;

/// Load a group or fail with the `NotFound`.
pub(crate) async fn fetch_group(queries: &Queries, id: Uuid) -> Result<Groups, V2Error> {
    queries.get_group_by_id(id).await.map_err(|e| match e {
        sqlx::Error::RowNotFound => V2Error::not_found("Group not found."),
        other => V2Error::from_sqlx(other),
    })
}

/// The caller's role in the group. BaaS hides a group the caller is not part
/// of behind the same `NotFound` as a group that does not exist.
pub(crate) async fn require_role(
    queries: &Queries,
    uid: Uuid,
    gid: Uuid,
    max_role: i16,
) -> Result<i16, V2Error> {
    let role = queries
        .get_group_edge_state(uid, gid)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => V2Error::not_found("Group not found or permission denied."),
            other => V2Error::from_sqlx(other),
        })?;
    if role > max_role {
        return Err(V2Error::not_found("Group not found or permission denied."));
    }
    Ok(role)
}
